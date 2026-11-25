-- The operation table is rebuilt to allow the executing and rolled back states, which a CHECK
-- cannot gain in place. Foreign keys are off while a migration runs, so the rows in
-- operation_rename and operation_tag_decision stay put.
CREATE TABLE operation_next (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,
    state TEXT NOT NULL CHECK (
        state IN ('pending', 'executing', 'applied', 'failed', 'undone', 'rolled_back')
    ),
    summary TEXT NOT NULL,
    forward_json TEXT NOT NULL,
    inverse_json TEXT NOT NULL,
    created_ns INTEGER NOT NULL,
    applied_ns INTEGER,
    undone_ns INTEGER,
    -- The step being worked on while the operation is executing.
    current_step INTEGER,
    -- When and how the operation was brought to rest after the app stopped in the middle of it.
    recovered_ns INTEGER,
    recovery TEXT
) STRICT;

INSERT INTO operation_next (
    id, kind, state, summary, forward_json, inverse_json, created_ns, applied_ns, undone_ns
)
SELECT id, kind, state, summary, forward_json, inverse_json, created_ns, applied_ns, undone_ns
FROM operation;

DROP TABLE operation;
ALTER TABLE operation_next RENAME TO operation;
CREATE INDEX operation_recent ON operation (created_ns DESC);
CREATE INDEX operation_unfinished ON operation (id) WHERE state IN ('pending', 'executing');

-- The plan of an operation made of file steps, written whole before the first file is touched.
-- `inverse_json` is what puts the step back.
CREATE TABLE operation_step (
    operation_id INTEGER NOT NULL REFERENCES operation (id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('planned', 'done', 'undone')),
    forward_json TEXT NOT NULL,
    inverse_json TEXT NOT NULL,
    PRIMARY KEY (operation_id, position)
) STRICT, WITHOUT ROWID;

-- A file moved to the bin stays in the library, left out of every view, until the operation
-- that moved it is undone.
ALTER TABLE asset ADD COLUMN trashed_by INTEGER REFERENCES operation (id);
