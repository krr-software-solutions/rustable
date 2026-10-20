/*
 * Append-only audit stream of everything that happens to an invoice.
 * Range-partitioned by quarter so old quarters can be detached and archived.
 */
CREATE TABLE billing.invoice_events (
    id          bigint      GENERATED ALWAYS AS IDENTITY,
    invoice_id  uuid        NOT NULL,
    kind        text        NOT NULL,
    payload     jsonb       NOT NULL DEFAULT '{}'::jsonb,
    occurred_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (id, occurred_at)
) PARTITION BY RANGE (occurred_at);

CREATE TABLE billing.invoice_events_2025_q2 PARTITION OF billing.invoice_events
    FOR VALUES FROM ('2025-04-01') TO ('2025-07-01');
CREATE TABLE billing.invoice_events_2025_q3 PARTITION OF billing.invoice_events
    FOR VALUES FROM ('2025-07-01') TO ('2025-10-01');
CREATE TABLE billing.invoice_events_default PARTITION OF billing.invoice_events DEFAULT;

CREATE INDEX invoice_events_invoice_id_idx ON billing.invoice_events (invoice_id, occurred_at);
