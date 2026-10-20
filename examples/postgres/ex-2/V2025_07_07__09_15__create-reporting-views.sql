-- Plain view: always fresh, used by the "outstanding" dashboard tile.
CREATE VIEW billing.open_invoices AS
SELECT i.id,
       i.tenant_id,
       i.number,
       i.amount_cents,
       i.currency,
       i.due_date,
       (i.due_date < current_date) AS is_overdue
  FROM billing.invoices i
 WHERE i.status = 'open';

-- Materialized: refreshed nightly by a cron job, too expensive to compute live.
CREATE MATERIALIZED VIEW billing.monthly_revenue AS
SELECT tenant_id,
       date_trunc('month', created_at) AS month,
       currency,
       sum(amount_cents)               AS revenue_cents,
       count(*)                        AS invoice_count
  FROM billing.invoices
 WHERE status = 'paid'
 GROUP BY tenant_id, date_trunc('month', created_at), currency
WITH DATA;

-- Required for REFRESH MATERIALIZED VIEW CONCURRENTLY.
CREATE UNIQUE INDEX monthly_revenue_key ON billing.monthly_revenue (tenant_id, month, currency);
