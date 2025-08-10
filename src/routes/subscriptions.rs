use crate::{
    domain::{NewSubscriber, SubscriberEmail, SubscriberName},
    email_client::EmailClient,
    startup::ApplicationBaseUrl,
};
use actix_web::http::StatusCode;
use actix_web::{
    HttpResponse, ResponseError,
    web::{Data, Form},
};
use chrono::Utc;
use rand::distributions::Alphanumeric;
use rand::{Rng, thread_rng};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct FormData {
    name: String,
    email: String,
}

// If you provide a TryFrom implementation, your type automatically gets the corresponding TryInto implementation
// hence can use try_into() instead of try_from()
impl TryFrom<FormData> for NewSubscriber {
    type Error = String;

    fn try_from(value: FormData) -> Result<Self, Self::Error> {
        let name = SubscriberName::parse(value.name)?;
        let email = SubscriberEmail::parse(value.email)?;
        Ok(Self { email, name })
    }
}

/// Generate a random 25-characters-long case-sensitive subscription token.
fn generate_subscription_token() -> String {
    let mut rng = thread_rng();
    std::iter::repeat_with(|| rng.sample(Alphanumeric))
        .map(char::from)
        .take(25)
        .collect()
}

#[tracing::instrument(
    name = "Adding a new subscriber",
    skip(form, pool, email_client, base_url),
    fields(
        subscriber_email = %form.email,
        subscriber_name= %form.name
    )
)]
pub async fn subscribe(
    form: Form<FormData>,
    pool: Data<PgPool>,
    email_client: Data<EmailClient>,
    base_url: Data<ApplicationBaseUrl>,
) -> Result<HttpResponse, SubscribeError> {
    let new_subscriber = form.0.try_into().map_err(SubscribeError::ValidationError)?;
    let mut transaction = pool.begin().await.map_err(|e| {
        SubscribeError::UnexpectedError(
            Box::new(e),
            "Failed to acquire a Postgres connection from the pool".into(),
        )
    })?;
    let subscriber_id = insert_subscriber(&mut transaction, &new_subscriber)
        .await
        .map_err(|e| {
            SubscribeError::UnexpectedError(
                Box::new(e),
                "Failed to insert new subscriber in the database.".into(),
            )
        })?;
    let subscription_token = generate_subscription_token();

    // store_token invokes 'Into' trait, so no need of map_err
    store_token(&mut transaction, subscriber_id, &subscription_token)
        .await
        .map_err(|e| {
            SubscribeError::UnexpectedError(
                Box::new(e),
                "Failed to store the confirmation token for a new subscriber.".into(),
            )
        })?;

    transaction.commit().await.map_err(|e| {
        SubscribeError::UnexpectedError(
            Box::new(e),
            "Failed to commit SQL transaction to store a new subscriber.".into(),
        )
    })?;

    send_confirmation_email(
        &email_client,
        new_subscriber,
        &base_url.0,
        &subscription_token,
    )
    .await
    .map_err(|e| {
        SubscribeError::UnexpectedError(Box::new(e), "Failed to send a confirmation email.".into())
    })?;

    Ok(HttpResponse::Ok().finish())
}

#[tracing::instrument(
    name = "Send a confirmation email to a new subscriber",
    skip(email_client, new_subscriber)
)]
pub async fn send_confirmation_email(
    email_client: &EmailClient,
    new_subscriber: NewSubscriber,
    base_url: &str,
    subscription_token: &str,
) -> Result<(), reqwest::Error> {
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        base_url, subscription_token
    );

    let plain_body = format!(
        "Welcome to our newsletter!\nVisit {} to confirm your subscription.",
        confirmation_link
    );
    let html_body = format!(
        "Welcome to our newsletter!<br />\
    Click <a href=\"{}\">here</a> to confirm your subscription.",
        confirmation_link
    );

    email_client
        .send_email(new_subscriber.email, "Welcome!", &html_body, &plain_body)
        .await
}

#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(new_subscriber, transaction)
)]
pub async fn insert_subscriber(
    transaction: &mut Transaction<'_, Postgres>,
    new_subscriber: &NewSubscriber,
) -> Result<Uuid, sqlx::Error> {
    let subscriber_id = Uuid::new_v4();

    sqlx::query!(
        r#"
            INSERT INTO subscriptions (id, email, name, subscribed_at, status)
            VALUES ($1, $2, $3, $4, 'pending_confirmation')
        "#,
        subscriber_id,
        new_subscriber.email.as_ref(),
        new_subscriber.name.as_ref(),
        Utc::now()
    )
    .execute(&mut **transaction)
    // The double dereference (**) gets us to the actual Transaction type, and then we take a mutable reference (&mut) to match the expected executor interface
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;

    Ok(subscriber_id)
}

#[tracing::instrument(
    name = "Store subscription token in the database",
    skip(subscription_token, transaction)
)]
pub async fn store_token(
    transaction: &mut Transaction<'_, Postgres>,
    subscriber_id: Uuid,
    subscription_token: &str,
) -> Result<(), StoreTokenError> {
    sqlx::query!(
        r#"INSERT INTO subscription_tokens (subscription_token, subscriber_id)
    VALUES ($1, $2)"#,
        subscription_token,
        subscriber_id
    )
    .execute(&mut **transaction)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        StoreTokenError(e)
    })?;

    Ok(())
}

// -----------------------------------------------------------------------------

// this is a wrapper around sqlx::Error - so that we can impl a foreign trait on it (orphan rule)
// This restriction is meant to preserve coherence: imagine if you added a dependency that defined its
// own implementation of ResponseError for sqlx::Error - which one should the compiler use when the trait methods are invoked?
pub struct StoreTokenError(sqlx::Error);

// could have just used derive(Debug) if we didn't want to show the custom message
// // instead of getting the default impl, we decided to write our own to make the relship between StoreTokenError and slqx::error more explicit
impl std::fmt::Debug for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // write!(f, "{}\nCaused by:\n\t{}", self, self.0) // self would return Display, self.0 would return actual error
        error_chain_fmt(self, f)
    }
}

// Display would be shown in exception.message while Debug would be shown in exception.details ie actual error - cant derive Display
impl std::fmt::Display for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "A database error was encountered while \
trying to store a subscription token."
        )
    }
}

// to get low level error details
impl std::error::Error for StoreTokenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // The compiler transparently casts `&sqlx::Error` into a `&dyn Error`
        Some(&self.0)
    }
}

// to make it compatible with actix_web::ResponseError trait
// impl ResponseError for StoreTokenError {}  // //REMOVING this because we're going to be creating another custom error type specifically for subscribe endpoint below

// for any type that implements std::error::Error, we can use this function to format the error chain
fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{}\n", e)?; //write the current error followed by empty line
    let mut current = e.source(); //grab the source
    while let Some(cause) = current {
        //check source exists
        writeln!(f, "Caused by: \n\t{}", cause)?; //if it does, write it as cause
        current = cause.source(); //grab its source next
    } //etc in a  loop
    Ok(())
}

// -----------------------------------------------------------------------------

// a custom error class purely for subscribe, not to mix concerns wiht other endpoints (they may wnat to display errors differently)

// AUTOMATIC USING A MACRO WITH ONLY NECESSARY FIELDS
// user does not care what the error was with db or email client, they just want to know that something went wrong, just the validation error is enough

#[derive(thiserror::Error)]
pub enum SubscribeError {
    #[error("{0}")]
    ValidationError(String),
    // Transparent delegates both `Display`'s and `source`'s implementation to the type wrapped by `UnexpectedError`.
    // #[error(transparent)]
    #[error("{1}")]
    // to add a custom message to the error - else could just use transparent and it printed the error::Error message
    UnexpectedError(#[source] Box<dyn std::error::Error>, String),
    // String is to add a custom message to the error
    // we wanted a type that can be used to wrap any error, so that we can use it in the UnexpectedError field
    // Box<dyn std::error::Error> is a trait object that can hold any error that implements the std::error::Error trait
}

impl std::fmt::Debug for SubscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for SubscribeError {
    fn status_code(&self) -> StatusCode {
        match self {
            SubscribeError::ValidationError(_) => StatusCode::BAD_REQUEST,
            SubscribeError::UnexpectedError(_, _) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

// // ---------------------------
// // AUTOMATIC USING A MACRO WITH DISTINCT TYPES FOR EACH ERROR

// #[derive(thiserror::Error)]
// pub enum SubscribeError {
//     #[error("{0}")]  // to put string in the error message - similar to self.0
//     ValidationError(String), //string doesn't implement the Error trait, therefore it can't be returned in Error:source

//     #[error("Failed to acquire a Postgres connection from the pool")]
//     PoolError(#[source] sqlx::Error),

//     #[error("Failed to insert new subscriber in the database.")]
//     InsertSubscriberError(#[source] sqlx::Error),

//     #[error("Failed to store the confirmation token for a new subscriber.")]
//     StoreTokenError(#[from] StoreTokenError), //from actually = from + source

//     #[error("Failed to commit SQL transaction to store a new subscriber.")]
//     TransactionCommitError(#[source] sqlx::Error),

//     #[error("Failed to send a confirmation email.")]
//     SendEmailError(#[from] reqwest::Error),
// }

// impl std::fmt::Debug for SubscribeError {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         error_chain_fmt(self, f)
//     }
// }

// impl ResponseError for SubscribeError {
//     fn status_code(&self) -> StatusCode {
//         match self {
//             SubscribeError::ValidationError(_) => StatusCode::BAD_REQUEST,
//             SubscribeError::PoolError(_)
//             | SubscribeError::TransactionCommitError(_)
//             | SubscribeError::InsertSubscriberError(_)
//             | SubscribeError::StoreTokenError(_)
//             | SubscribeError::SendEmailError(_) => StatusCode::INTERNAL_SERVER_ERROR,
//         }
//     }
// }

// // ---------------------------
// // MANUAL APPROACH

// // overall we're doing 2 things:
// // 1)preparing a ResponseError for the api
// // 2)provider relevant diagnostic (source, debug, display) for the human

// // by using an enum + from we can get rid of all the map_err in our code

// pub enum SubscribeError {
//     ValidationError(String),
//     // DatabaseError(sqlx::Error),
//     StoreTokenError(StoreTokenError),
//     SendEmailError(reqwest::Error),
//     PoolError(sqlx::Error),
//     InsertSubscriberError(sqlx::Error),
//     TransactionCommitError(sqlx::Error),
// }

// impl std::fmt::Debug for SubscribeError {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         error_chain_fmt(self, f)
//     }
// }

// impl std::fmt::Display for SubscribeError {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         match self {
//             SubscribeError::ValidationError(e) => write!(f, "{}", e),
//             // SubscribeError::DatabaseError(_) => write!(f, "???"),
//             SubscribeError::StoreTokenError(_) => write!(
//                 f,
//                 "Failed to store the confirmation token for a new subscriber."
//             ),
//             SubscribeError::SendEmailError(_) => {
//                 write!(f, "Failed to send a confirmation email.")
//             }
//             SubscribeError::PoolError(_) => {
//                 write!(f, "Failed to acquire a Postgres connection from the pool")
//             }
//             SubscribeError::InsertSubscriberError(_) => {
//                 write!(f, "Failed to insert new subscriber in the database.")
//             }
//             SubscribeError::TransactionCommitError(_) => {
//                 write!(
//                     f,
//                     "Failed to commit SQL transaction to store a new subscriber."
//                 )
//             }
//         }
//     }
// }

// impl std::error::Error for SubscribeError {
//     fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
//         match self {
//             // &str does not implement `Error` - we consider it the root cause
//             SubscribeError::ValidationError(_) => None,
//             // SubscribeError::DatabaseError(e) => Some(e),
//             SubscribeError::StoreTokenError(e) => Some(e),
//             SubscribeError::SendEmailError(e) => Some(e),
//             SubscribeError::PoolError(e) => Some(e),
//             SubscribeError::InsertSubscriberError(e) => Some(e),
//             SubscribeError::TransactionCommitError(e) => Some(e),
//         }
//     }
// }

// impl ResponseError for SubscribeError {
//     fn status_code(&self) -> StatusCode {
//         match self {
//             SubscribeError::ValidationError(_) => StatusCode::BAD_REQUEST,
//             SubscribeError::PoolError(_)
//             | SubscribeError::TransactionCommitError(_)
//             | SubscribeError::InsertSubscriberError(_)
//             | SubscribeError::StoreTokenError(_)
//             | SubscribeError::SendEmailError(_) => StatusCode::INTERNAL_SERVER_ERROR,
//         }
//     }
// }

// // impl std::error::Error for SubscribeError {}
// // impl ResponseError for SubscribeError {}

// impl From<reqwest::Error> for SubscribeError {
//     fn from(e: reqwest::Error) -> Self {
//         Self::SendEmailError(e)
//     }
// }

// impl From<StoreTokenError> for SubscribeError {
//     fn from(e: StoreTokenError) -> Self {
//         Self::StoreTokenError(e)
//     }
// }

// impl From<String> for SubscribeError {
//     fn from(e: String) -> Self {
//         Self::ValidationError(e)
//     }
// }

//subscribe before we re-instrumented tracing
// pub async fn subscribe(
//     form: web::Form<FormData>,
//     // "dependency injection"
//     // connection: web::Data<Arc<PgConnection>>, //Data is an extractor - extracts whatever is stored under type <Arc<PgConnection>> in data
//     pg_pool: web::Data<PgPool>,
// ) -> Result<HttpResponse, HttpResponse> {
//     let request_id = Uuid::new_v4();
//     //spans like logs have an associated level
//     let request_span = tracing::info_span!(
//         "Adding new subscriber",
//         //we're adding strucutre info
//         //using % to tell tracing to use their Display impl for logging purposes
//         %request_id, //implicit naming - use variable name for its key
//         email = %form.email,
//         name = %form.name
//     );
//     //not enough to create the span, we also need to enter it
//     //manual way below, but that's not how we want to do it. we want to use Instrument, so that span auto opens/closes on async actions
//     let _request_span_guard = request_span.enter();
//
//     let query_span = tracing::info_span!("Saving new subscriber to db.");
//
//     sqlx::query!(
//         r#"
//         INSERT INTO subscriptions (id, email, name, subscribed_at)
//         VALUES ($1, $2, $3, $4)
//         "#,
//         Uuid::new_v4(),
//         form.email,
//         form.name,
//         Utc::now()
//     )
//     // web::Data<Arc<PgConnection>> is equivalent to Arc<Arc<PgConnection>>
//     // so to get it we first do get_ref >  &Arc<PgConnection>, then deref() to get &PgConnection
//     // .deref() - discussed here https://doc.rust-lang.org/stable/book/ch15-02-deref.html - anything that has deref implemented on it can be used to extract the inner something. &Arc<something> -> &something
//     // .get_ref() - seems to be specific to actix, I couldn't find it in general docs - https://docs.rs/actix-web/4.0.0-beta.3/actix_web/web/struct.Data.html#method.get_ref
//     // .execute(connection.get_ref().deref())
//     // this time with pg_pool we only unwrap once
//     .execute(pg_pool.get_ref())
//     .instrument(query_span) //exits the span every time the future is parked
//     .await
//     //map_err - coerces one type of error into another by applying a function (in this case closure) to it -https://doc.rust-lang.org/std/result/enum.Result.html#method.map_err
//     .map_err(|e| {
//         tracing::error!(
//             "request_id: {}, failed to execute query {:?}",
//             request_id,
//             e
//         );
//         HttpResponse::InternalServerError().finish()
//     })?;
//     // tracing::info!(
//     //     "done saving new subscribed to db, request_id: {}",
//     //     request_id
//     // );
//
//     Ok(HttpResponse::Ok().finish())
// }

//using standard logging instead of tracing
// pub async fn subscribe(
//     form: web::Form<FormData>,
//     // "dependency injection"
//     // connection: web::Data<Arc<PgConnection>>, //Data is an extractor - extracts whatever is stored under type <Arc<PgConnection>> in data
//     pg_pool: web::Data<PgPool>,
// ) -> Result<HttpResponse, HttpResponse> {
//     let request_id = Uuid::new_v4();
//     log::info!(
//         ">> request_id: {}, saving {}, {} as new subscriber to db",
//         request_id,
//         form.email,
//         form.name
//     );
//     sqlx::query!(
//         r#"
//         INSERT INTO subscriptions (id, email, name, subscribed_at)
//         VALUES ($1, $2, $3, $4)
//         "#,
//         Uuid::new_v4(),
//         form.email,
//         form.name,
//         Utc::now()
//     )
//     // web::Data<Arc<PgConnection>> is equivalent to Arc<Arc<PgConnection>>
//     // so to get it we first do get_ref >  &Arc<PgConnection>, then deref() to get &PgConnection
//     // .deref() - discussed here https://doc.rust-lang.org/stable/book/ch15-02-deref.html - anything that has deref implemented on it can be used to extract the inner something. &Arc<something> -> &something
//     // .get_ref() - seems to be specific to actix, I couldn't find it in general docs - https://docs.rs/actix-web/4.0.0-beta.3/actix_web/web/struct.Data.html#method.get_ref
//     // .execute(connection.get_ref().deref())
//     // this time with pg_pool we only unwrap once
//     .execute(pg_pool.get_ref())
//     .await
//     //map_err - coerces one type of error into another by applying a function (in this case closure) to it -https://doc.rust-lang.org/std/result/enum.Result.html#method.map_err
//     .map_err(|e| {
//         log::error!(
//             ">> request_id: {}, failed to execute query {:?}",
//             request_id,
//             e
//         );
//         HttpResponse::InternalServerError().finish()
//     })?;
//     log::info!(
//         ">> done saving new subscribed to db, request_id: {}",
//         request_id
//     );
//
//     Ok(HttpResponse::Ok().finish())
// }
