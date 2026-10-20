-- Mise: a tiny personal recipe box.
CREATE TABLE recipes (
    id           bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name         text        NOT NULL,
    servings     integer     NOT NULL DEFAULT 2 CHECK (servings > 0),
    tags         text[]      NOT NULL DEFAULT ARRAY[]::text[],
    ingredients  jsonb       NOT NULL DEFAULT '[]'::jsonb,
    instructions text,
    created_at   timestamptz NOT NULL DEFAULT now()
);

-- Case-insensitive name lookup (expression index).
CREATE UNIQUE INDEX recipes_name_lower_key ON recipes (lower(name));

-- "Which recipes use chickpeas?" -> ingredients @> '[{"item":"chickpeas"}]'
CREATE INDEX recipes_ingredients_gin_idx ON recipes USING gin (ingredients jsonb_path_ops);

INSERT INTO recipes (name, servings, tags, ingredients) VALUES
    ('Weeknight Chana Masala', 4, ARRAY['vegan', 'curry'],
     '[{"item": "chickpeas", "qty": "2 cans"}, {"item": "onion", "qty": "1"}]'),
    ('Shakshuka', 2, ARRAY['vegetarian', 'brunch'],
     '[{"item": "eggs", "qty": "4"}, {"item": "tomatoes", "qty": "1 can"}]');
