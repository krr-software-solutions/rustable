-- Ledgerly: multi-tenant invoicing. Every row in the system hangs off a tenant.
-- gen_random_uuid() is core since PG13, but we still pin pgcrypto for older replicas.
CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE tenants (
    id         uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    slug       text        NOT NULL UNIQUE CHECK (slug ~ '^[a-z0-9-]{3,40}$'),
    name       text        NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
