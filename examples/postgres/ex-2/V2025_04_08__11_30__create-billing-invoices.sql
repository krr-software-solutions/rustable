CREATE SCHEMA billing;

CREATE TYPE billing.invoice_status AS ENUM ('draft', 'open', 'paid');

CREATE TABLE billing.invoices (
    id           uuid                   PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    uuid                   NOT NULL REFERENCES public.tenants (id) ON DELETE CASCADE,
    number       text                   NOT NULL,
    amount_cents bigint                 NOT NULL CHECK (amount_cents >= 0),
    currency     char(3)                NOT NULL DEFAULT 'USD',
    status       billing.invoice_status NOT NULL DEFAULT 'draft',
    metadata     jsonb                  NOT NULL DEFAULT '{}'::jsonb,
    created_at   timestamptz            NOT NULL DEFAULT now(),
    CONSTRAINT invoices_tenant_number_key UNIQUE (tenant_id, number)
);

CREATE INDEX invoices_tenant_status_idx ON billing.invoices (tenant_id, status);

COMMENT ON SCHEMA billing IS 'Everything money-related lives here.';
COMMENT ON COLUMN billing.invoices.amount_cents IS 'Total in minor units of currency.';
