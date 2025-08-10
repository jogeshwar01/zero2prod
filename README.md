## Email Newsletter API 📧

Reference implementation from the <a href="https://www.zero2prod.com/">Zero To Production</a> book by Luca Palmieri

### Features

- 📧 **Email Newsletter Management** – Subscribe, confirm, and send newsletters to all confirmed subscribers
- 🔒 **Email Validation** – Robust email validation and confirmation flow
- 🏗️ **Production-Ready Architecture** – Built following production best practices
- 📊 **Comprehensive Logging** – Structured logging with tracing and telemetry
- 🗄️ **Database Integration** – PostgreSQL with SQLx for type-safe queries
- ⚡ **Async/Await** – High-performance async web server with Actix Web
- 🧪 **Extensive Testing** – Integration tests covering all API endpoints
- 🔧 **Configuration Management** – Environment-based configuration with YAML files

### Tech Stack

- [Rust](https://www.rust-lang.org/) – Systems programming language for performance and safety
- [Actix Web](https://actix.rs/) – Powerful, pragmatic, and fast web framework
- [SQLx + PostgreSQL](https://github.com/launchbadge/sqlx) – Async SQL toolkit with compile-time checked queries and PostgreSQL database
- [Tokio](https://tokio.rs/) – Asynchronous runtime for Rust
- [Tracing](https://tracing.rs/) – Application-level tracing framework
- [Serde](https://serde.rs/) – Serialization framework for Rust
- [Thiserror + Anyhow](https://docs.rs/thiserror/) – Comprehensive error handling and context

#### Key Dependencies

**Core Web Framework:**

- `actix-web` - Web server and HTTP handling
- `tokio` - Async runtime with multi-threading support
- `tracing` + `tracing-actix-web` - Structured logging and observability

**Database & Persistence:**

- `sqlx` - Type-safe SQL with async support and migrations
- `uuid` - Unique identifier generation for subscribers and tokens

**Error Handling:**

- `thiserror` - Ergonomic derive macros for custom error types
- `anyhow` - Flexible concrete Error type for application errors

**Serialization & Validation:**

- `serde` - JSON serialization/deserialization
- `validator` - Email and data validation
- `unicode-segmentation` - Proper Unicode string handling

**Configuration & Security:**

- `config` - Multi-environment configuration management
- `secrecy` - Secret handling to prevent accidental leaks

**Email & HTTP Client:**

- `reqwest` - HTTP client for external email service integration
- `chrono` - Date and time handling

**Testing & Development:**

- `wiremock` - HTTP mocking for integration tests
- `fake` - Fake data generation for testing
- `claim` - Additional assertion macros

### API Endpoints

- `GET /health_check` → Service health status
- `POST /subscriptions` → Subscribe a new email to the newsletter
- `GET /subscriptions/confirm` → Confirm email subscription via token
- `POST /newsletters` → Send newsletter to all confirmed subscribers

### Local Development

#### Prerequisites

- Rust (latest stable)
- PostgreSQL
- Docker (optional, for containerized database)

#### Setup

1. **Clone the repository**

```bash
git clone https://github.com/jogeshwar01/zero2prod
cd zero2prod
```

2. **Set up the database**

```bash
# Using the provided script
./scripts/init_db.sh
```

3. **Run database migrations**

```bash
sqlx database create
sqlx migrate run
```

4. **Build and run**

```bash
cargo build
cargo run
```

5. **Run tests**

```bash
cargo test
```

The server will start on `http://localhost:8000` by default.

#### Configuration

```yaml
application:
  port: 8000
database:
  host: "localhost"
  port: 5440
  username: "postgres"
  password: "password"
  database_name: "newsletter"
email_client:
  base_url: "localhost"
  sender_email: "test@gmail.com"
  authorization_token: "my-secret-token"
  timeout_milliseconds: 10000
```

### Project Structure

```
zero2prod/
├── Cargo.toml              # Project dependencies and metadata
├── Cargo.lock              # Dependency lock file
├── README.md               # Project documentation
├── Dockerfile              # Container configuration
├── config/                 # Configuration files
├── migrations/             # Database migration files
|── scripts/
│   └── init_db.sh          # Database initialization script
├── src/
│   ├── main.rs             # Application entry point
│   ├── lib.rs              # Library root
│   ├── startup.rs          # Application and server setup
│   ├── configuration.rs    # Configuration management
│   ├── telemetry.rs        # Logging and tracing setup
│   ├── email_client.rs     # Email service client
│   ├── domain/             # Business logic and domain models
│   │   ├── mod.rs
│   │   ├── new_subscriber.rs
│   │   ├── subscriber_email.rs
│   │   └── subscriber_name.rs
│   └── routes/             # HTTP route handlers
│       ├── mod.rs
│       ├── health_check.rs
│       ├── subscriptions.rs
│       ├── subscriptions_confirm.rs
│       └── newsletter.rs
└── tests/                  # Integration tests
    └── api/
        ├── main.rs
        ├── helpers.rs
        ├── health_check.rs
        ├── subscriptions.rs
        ├── subscriptions_confirm.rs
        └── newsletter.rs
```
