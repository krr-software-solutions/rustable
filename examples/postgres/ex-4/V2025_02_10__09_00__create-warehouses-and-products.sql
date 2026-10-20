-- Stockroom: inventory tracking for a handful of small warehouses.
CREATE TABLE warehouses (
    id         integer     GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code       char(4)     NOT NULL UNIQUE CHECK (code = upper(code)),
    city       text        NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE products (
    id          bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name        text        NOT NULL,
    description text        NOT NULL DEFAULT '',
    price_cents bigint      NOT NULL CHECK (price_cents >= 0),
    created_at  timestamptz NOT NULL DEFAULT now()
);

-- Free-form notes the ops team used before tickets existed. Slated for removal.
CREATE TABLE warehouse_notes (
    id           bigserial PRIMARY KEY,
    warehouse_id integer   NOT NULL REFERENCES warehouses (id),
    body         text      NOT NULL
);

COMMENT ON TABLE products IS 'Catalogue of sellable items, independent of location.';

INSERT INTO warehouses (code, city) VALUES ('BER1', 'Berlin'), ('LIS1', 'Lisbon');
INSERT INTO products (name, price_cents) VALUES ('Desk lamp', 3499), ('Cable tray', 1250);
