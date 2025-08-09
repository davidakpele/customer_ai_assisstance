FROM rust:latest

WORKDIR /app

COPY . .

RUN cargo build --release

EXPOSE 8055

CMD sqlx migrate run && ./target/release/ai_web_assistant
