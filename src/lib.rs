use std::net::TcpListener;

use actix_web::{
    App, HttpResponse, HttpServer,
    dev::Server,
    web::{self, Form},
};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct FormData {
    name: String,
    email: String,
}

async fn health_check() -> HttpResponse {
    HttpResponse::Ok().finish()
}

async fn subscribe(data: Form<FormData>) -> HttpResponse {
    let data = data.into_inner();
    let (name, email) = (data.name, data.email);
    println!("{} - {}", name, email);

    HttpResponse::Ok().finish()
}

pub fn run(listener: TcpListener) -> Result<Server, std::io::Error> {
    let server = HttpServer::new(|| {
        App::new()
            .route("/health_check", web::get().to(health_check))
            .route("/subscriptions", web::post().to(subscribe))
    })
    .listen(listener)?
    .run();

    Ok(server)
}
