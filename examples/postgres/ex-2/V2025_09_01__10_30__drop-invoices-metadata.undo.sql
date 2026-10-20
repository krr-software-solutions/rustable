-- Structure only: the original JSON payloads are gone for good.
ALTER TABLE billing.invoices ADD COLUMN metadata jsonb NOT NULL DEFAULT '{}'::jsonb;
