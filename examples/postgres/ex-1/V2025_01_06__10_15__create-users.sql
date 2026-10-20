-- Pinboard: the very first table. Users sign up with an email address only.
CREATE TABLE users (
    id           bigserial PRIMARY KEY,
    email        text        NOT NULL,
    display_name text,
    created_at   timestamptz NOT NULL DEFAULT now()
);

-- Emails are compared case-insensitively, so enforce uniqueness on lower(email).
CREATE UNIQUE INDEX users_email_lower_key ON users (lower(email));
