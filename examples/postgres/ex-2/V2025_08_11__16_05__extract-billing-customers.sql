/*
 * Customers used to live as free-form JSON in invoices.metadata.
 * Promote them to a real table and link invoices to it.
 */
CREATE TABLE billing.customers (
    id         uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  uuid        NOT NULL REFERENCES public.tenants (id) ON DELETE CASCADE,
    email      text        NOT NULL,
    name       text,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, email)
);

INSERT INTO billing.customers (tenant_id, email, name)
SELECT DISTINCT ON (i.tenant_id, lower(i.metadata ->> 'customer_email'))
       i.tenant_id,
       lower(i.metadata ->> 'customer_email'),
       i.metadata ->> 'customer_name'
  FROM billing.invoices i
 WHERE i.metadata ? 'customer_email'
 ORDER BY i.tenant_id, lower(i.metadata ->> 'customer_email'), i.created_at DESC;

ALTER TABLE billing.invoices ADD COLUMN customer_id uuid;

UPDATE billing.invoices i
   SET customer_id = c.id
  FROM billing.customers c
 WHERE c.tenant_id = i.tenant_id
   AND c.email = lower(i.metadata ->> 'customer_email');

ALTER TABLE billing.invoices
    ADD CONSTRAINT invoices_customer_id_fkey
    FOREIGN KEY (customer_id) REFERENCES billing.customers (id) NOT VALID;
ALTER TABLE billing.invoices VALIDATE CONSTRAINT invoices_customer_id_fkey;
