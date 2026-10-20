CREATE TABLE bookmarks (
    id         bigserial PRIMARY KEY,
    user_id    bigint      NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    url        text        NOT NULL CHECK (url ~* '^https?://'),
    title      text        NOT NULL,
    tags       text[]      NOT NULL DEFAULT '{}',
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX bookmarks_user_id_idx ON bookmarks (user_id);
CREATE INDEX bookmarks_tags_gin_idx ON bookmarks USING gin (tags);
