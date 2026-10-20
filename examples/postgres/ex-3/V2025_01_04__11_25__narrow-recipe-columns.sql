-- The mobile UI truncates names at 120 chars and nobody cooks for 32k people.
-- Narrowing is lossy: long names are cut, and huge servings would fail the cast.
ALTER TABLE recipes
    ALTER COLUMN name     SET DATA TYPE varchar(120) USING left(name, 120),
    ALTER COLUMN servings SET DATA TYPE smallint;

ALTER TABLE recipes
    ADD CONSTRAINT recipes_servings_reasonable CHECK (servings <= 100);
