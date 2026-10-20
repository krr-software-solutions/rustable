/*
 * Soft-archiving: instead of deleting, users can archive a bookmark.
 * The column is nullable so existing app code keeps working untouched.
 */
ALTER TABLE bookmarks ADD COLUMN archived_at timestamptz;

-- The home feed only ever lists live bookmarks, newest first.
-- CONCURRENTLY: cannot run inside a transaction block.
CREATE INDEX CONCURRENTLY bookmarks_live_feed_idx
    ON bookmarks (user_id, created_at DESC)
    WHERE archived_at IS NULL;
