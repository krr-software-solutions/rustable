-- Defence in depth: even a buggy query cannot read another tenant's invoices.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'ledgerly_app') THEN
        CREATE ROLE ledgerly_app NOLOGIN;
    END IF;
END
$$;

GRANT USAGE ON SCHEMA billing TO ledgerly_app;
GRANT SELECT, INSERT, UPDATE ON billing.invoices TO ledgerly_app;

ALTER TABLE billing.invoices ENABLE ROW LEVEL SECURITY;

CREATE POLICY invoices_tenant_isolation ON billing.invoices
    FOR ALL
    TO ledgerly_app
    USING (tenant_id = current_setting('app.tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.tenant_id', true)::uuid);
