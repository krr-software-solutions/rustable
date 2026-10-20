-- Product decided "title" is user-editable and should be called "label".
-- Breaking: any code still selecting bookmarks.title will fail.
ALTER TABLE bookmarks RENAME COLUMN title TO label;

COMMENT ON COLUMN bookmarks.label IS 'User-editable display label; defaults to the page <title> on save.';
