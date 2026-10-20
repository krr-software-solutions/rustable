CREATE TABLE stock_levels (
    warehouse_id integer     NOT NULL REFERENCES warehouses (id) ON DELETE CASCADE,
    product_id   bigint      NOT NULL REFERENCES products (id) ON DELETE CASCADE,
    quantity     integer     NOT NULL CHECK (quantity >= 0),
    reorder_at   integer     CHECK (reorder_at IS NULL OR reorder_at >= 0),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (warehouse_id, product_id)
);

-- Low-stock report: only rows that actually have a reorder threshold.
CREATE INDEX stock_levels_low_stock_idx
    ON stock_levels (warehouse_id, product_id)
    WHERE reorder_at IS NOT NULL;
