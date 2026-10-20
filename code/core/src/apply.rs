//! Fold statements into a schema.
//!
//! [`apply_statement`] looks at one statement and the current schema, works out the
//! [`Change`]s it makes, applies them, and returns them. Anything it can't make sense of becomes
//! a [`Change::Unknown`] instead of an error, so a run never stops halfway through a file.

use sqlparser::ast::{
    AlterColumnOperation, AlterIndexOperation, AlterTableOperation, AlterTypeOperation, ColumnDef,
    ColumnOption, CommentObject, CreateIndex, CreateTable, CreateTableOptions, IndexColumn,
    MySQLColumnPosition, ObjectName, ObjectType, RenameTableNameKind, SchemaName, SqlOption,
    Statement, TableConstraint, UserDefinedTypeRepresentation,
};

use crate::change::Change;
use crate::dialect::Dialect;
use crate::names::{ident, object_name, unqualified};
use crate::schema::{Check, Column, EnumType, ForeignKey, Index, OtherObject, Schema, Table, View};
use crate::statements::SqlStatement;

/// Work out what `stmt` does to `schema`, apply it, and return the changes.
pub fn apply_statement(schema: &mut Schema, stmt: &SqlStatement, dialect: Dialect) -> Vec<Change> {
    let changes = match &stmt.parsed {
        Ok(parsed) => Folder {
            schema,
            dialect,
            sql: &stmt.sql,
        }
        .statement(parsed),
        Err(error) => crate::heuristics::unparsed(schema, &stmt.sql, dialect)
            .unwrap_or_else(|| vec![classify_unparsed(stmt, error)]),
    };
    for change in &changes {
        change.apply_to(schema);
    }
    changes
}

/// The changes a parsed statement would make to `schema` (without applying them).
pub(crate) fn changes_for(
    schema: &Schema,
    parsed: &Statement,
    dialect: Dialect,
    sql: &str,
) -> Vec<Change> {
    Folder {
        schema,
        dialect,
        sql,
    }
    .statement(parsed)
}

/// Apply a whole list of statements, returning every change in order.
pub fn apply_all(schema: &mut Schema, stmts: &[SqlStatement], dialect: Dialect) -> Vec<Change> {
    stmts
        .iter()
        .flat_map(|s| apply_statement(schema, s, dialect))
        .collect()
}

/// Statements `sqlparser` couldn't parse: classify by their leading keyword.
fn classify_unparsed(stmt: &SqlStatement, error: &str) -> Change {
    let sql = stmt.sql.clone();
    match by_keyword(&stmt.leading_keyword()) {
        Kind::Data => Change::Data { sql },
        Kind::Session => Change::Session { sql },
        Kind::Ddl => Change::Unknown {
            sql,
            reason: format!("could not parse: {error}"),
        },
    }
}

enum Kind {
    Data,
    Session,
    Ddl,
}

fn by_keyword(keyword: &str) -> Kind {
    match keyword {
        "INSERT" | "UPDATE" | "DELETE" | "MERGE" | "REPLACE" | "TRUNCATE" | "COPY" | "WITH"
        | "SELECT" | "CALL" | "REFRESH" | "LOAD" => Kind::Data,
        "SET" | "PRAGMA" | "BEGIN" | "START" | "COMMIT" | "ROLLBACK" | "END" | "SAVEPOINT"
        | "RELEASE" | "USE" | "LOCK" | "UNLOCK" | "ANALYZE" | "VACUUM" | "SHOW" | "EXPLAIN"
        | "RESET" | "DISCARD" | "CHECKPOINT"
        // Permissions aren't part of the schema rustable models.
        | "GRANT" | "REVOKE" => Kind::Session,
        _ => Kind::Ddl,
    }
}

struct Folder<'a> {
    schema: &'a Schema,
    dialect: Dialect,
    sql: &'a str,
}

impl Folder<'_> {
    fn unknown(&self, reason: impl Into<String>) -> Vec<Change> {
        vec![Change::Unknown {
            sql: self.sql.to_string(),
            reason: reason.into(),
        }]
    }

    fn name(&self, name: &ObjectName) -> String {
        object_name(name, self.dialect)
    }

    fn statement(&self, stmt: &Statement) -> Vec<Change> {
        match stmt {
            Statement::CreateTable(ct) => self.create_table(ct),
            Statement::AlterTable(at) => {
                self.alter_table(&self.name(&at.name), &at.operations, at.if_exists)
            }
            Statement::CreateIndex(ci) => self.create_index(ci),
            Statement::Drop {
                object_type,
                names,
                if_exists,
                table,
                cascade,
                ..
            } => names
                .iter()
                .flat_map(|n| self.drop(*object_type, n, *if_exists, *cascade, table.as_ref()))
                .collect(),
            Statement::AlterIndex {
                name,
                operation: AlterIndexOperation::RenameIndex { index_name },
            } => {
                let from = unqualified(&self.name(name)).to_string();
                let to = unqualified(&self.name(index_name)).to_string();
                match self.schema.table_with_index(&from) {
                    Some(t) => vec![Change::RenameConstraint {
                        table: t.name.clone(),
                        from,
                        to,
                    }],
                    None => self.unknown(format!("rename of unknown index `{from}`")),
                }
            }
            Statement::RenameTable(renames) => renames
                .iter()
                .flat_map(|r| self.rename_table(&self.name(&r.old_name), &self.name(&r.new_name)))
                .collect(),
            Statement::CreateView(cv) => {
                let name = self.name(&cv.name);
                let replaced = self.schema.views.iter().find(|v| v.name == name).cloned();
                if replaced.is_some() && cv.if_not_exists {
                    return vec![];
                }
                vec![Change::CreateView {
                    view: View {
                        name,
                        materialized: cv.materialized,
                        query: cv.query.to_string(),
                    },
                    replaced,
                }]
            }
            Statement::CreateType {
                name,
                representation,
            } => {
                let name = self.name(name);
                match representation {
                    Some(UserDefinedTypeRepresentation::Enum { labels }) => {
                        vec![Change::CreateEnum {
                            enum_type: EnumType {
                                name,
                                values: labels.iter().map(|l| l.value.clone()).collect(),
                            },
                        }]
                    }
                    _ => self.create_object("type", name, None, false),
                }
            }
            Statement::AlterType(at) => {
                let name = self.name(&at.name);
                match &at.operation {
                    AlterTypeOperation::AddValue(add) => vec![Change::AddEnumValue {
                        name,
                        value: add.value.value.clone(),
                    }],
                    _ => self.unknown("ALTER TYPE other than ADD VALUE is not modelled"),
                }
            }
            Statement::CreateExtension(ext) => {
                self.create_object("extension", ident(&ext.name), None, ext.if_not_exists)
            }
            Statement::DropExtension(drop) => drop
                .names
                .iter()
                .flat_map(|n| self.drop_object("extension", &ident(n), drop.if_exists))
                .collect(),
            Statement::CreateSchema {
                schema_name,
                if_not_exists,
                ..
            } => {
                let name = match schema_name {
                    SchemaName::Simple(n) | SchemaName::NamedAuthorization(n, _) => self.name(n),
                    SchemaName::UnnamedAuthorization(i) => ident(i),
                };
                self.create_object("schema", name, None, *if_not_exists)
            }
            Statement::CreateFunction(func) => {
                self.create_object("function", self.name(&func.name), None, func.if_not_exists)
            }
            Statement::DropFunction(drop) => drop
                .func_desc
                .iter()
                .flat_map(|d| self.drop_object("function", &self.name(&d.name), drop.if_exists))
                .collect(),
            Statement::CreateTrigger(trigger) => self.create_object(
                "trigger",
                self.name(&trigger.name),
                Some(self.name(&trigger.table_name)),
                false,
            ),
            Statement::DropTrigger(drop) => {
                self.drop_object("trigger", &self.name(&drop.trigger_name), drop.if_exists)
            }
            Statement::CreatePolicy(policy) => self.create_object(
                "policy",
                ident(&policy.name),
                Some(self.name(&policy.table_name)),
                false,
            ),
            Statement::DropPolicy(drop) => {
                self.drop_object("policy", &ident(&drop.name), drop.if_exists)
            }
            Statement::CreateSequence {
                name,
                if_not_exists,
                ..
            } => self.create_object("sequence", self.name(name), None, *if_not_exists),
            Statement::Comment {
                object_type,
                object_name: target,
                comment,
                ..
            } => self.comment(*object_type, target, comment.clone()),
            _ => {
                if let Some(changes) =
                    crate::heuristics::by_words(self.schema, self.sql, self.dialect)
                {
                    return changes;
                }
                let keyword = crate::statements::SqlStatement {
                    line: 0,
                    sql: self.sql.to_string(),
                    parsed: Err(String::new()),
                }
                .leading_keyword();
                match by_keyword(&keyword) {
                    Kind::Data => vec![Change::Data {
                        sql: self.sql.to_string(),
                    }],
                    Kind::Session => vec![Change::Session {
                        sql: self.sql.to_string(),
                    }],
                    Kind::Ddl => self.unknown(format!("{keyword} statements are not modelled")),
                }
            }
        }
    }

    // ---- objects ------------------------------------------------------------------------------

    fn create_object(
        &self,
        kind: &str,
        name: String,
        table: Option<String>,
        if_not_exists: bool,
    ) -> Vec<Change> {
        let exists = self
            .schema
            .objects
            .iter()
            .any(|o| o.kind == kind && o.name == name);
        if exists && if_not_exists {
            return vec![];
        }
        vec![Change::CreateObject {
            object: OtherObject {
                kind: kind.to_string(),
                name,
                table,
            },
            replaced: exists,
        }]
    }

    fn drop_object(&self, kind: &str, name: &str, if_exists: bool) -> Vec<Change> {
        match self
            .schema
            .objects
            .iter()
            .find(|o| o.kind == kind && o.name == name)
        {
            Some(object) => vec![Change::DropObject {
                object: object.clone(),
            }],
            None if if_exists => vec![],
            None => self.unknown(format!("drop of unknown {kind} `{name}`")),
        }
    }

    // ---- tables -------------------------------------------------------------------------------

    fn create_table(&self, ct: &CreateTable) -> Vec<Change> {
        let name = self.name(&ct.name);
        if self.schema.table(&name).is_some() {
            return if ct.if_not_exists {
                vec![]
            } else {
                self.unknown(format!("table `{name}` already exists"))
            };
        }
        if ct.query.is_some() || ct.like.is_some() || ct.clone.is_some() {
            return self.unknown("CREATE TABLE … AS / LIKE / CLONE is not modelled");
        }

        let mut table = Table::new(name.clone());
        for def in &ct.columns {
            let (column, extras) = self.column_def(def);
            table.columns.push(column);
            for extra in extras {
                add_constraint(&mut table, extra);
            }
        }
        for constraint in &ct.constraints {
            match self.table_constraint(constraint) {
                Some(c) => add_constraint(&mut table, c),
                None => return self.unknown(format!("unsupported table constraint: {constraint}")),
            }
        }

        table.options = table_options(&ct.table_options);
        // MySQL puts the table COMMENT among the table options.
        if let Some(comment) = table.options.remove("COMMENT") {
            table.comment = Some(comment);
        }
        if ct.strict {
            table.options.insert("STRICT".into(), "true".into());
        }
        if ct.without_rowid {
            table.options.insert("WITHOUT ROWID".into(), "true".into());
        }
        if let Some(p) = &ct.partition_by {
            table.options.insert("PARTITION BY".into(), p.to_string());
        }
        if let Some(parent) = &ct.partition_of {
            table
                .options
                .insert("PARTITION OF".into(), self.name(parent));
        }
        if let Some(fv) = &ct.for_values {
            table.options.insert("FOR VALUES".into(), fv.to_string());
        }
        if let Some(comment) = &ct.comment {
            table.comment = Some(comment_text(&comment.to_string()));
        }
        fill_default_names(&mut table, self.dialect);
        vec![Change::CreateTable { table }]
    }

    fn rename_table(&self, from: &str, to: &str) -> Vec<Change> {
        if self.schema.table(from).is_none() {
            return self.unknown(format!("rename of unknown table `{from}`"));
        }
        vec![Change::RenameTable {
            from: from.to_string(),
            to: to.to_string(),
        }]
    }

    #[allow(clippy::too_many_lines)]
    fn alter_table(&self, name: &str, ops: &[AlterTableOperation], if_exists: bool) -> Vec<Change> {
        let Some(original) = self.schema.table(name) else {
            return if if_exists {
                vec![]
            } else {
                self.unknown(format!("alter of unknown table `{name}`"))
            };
        };
        // Operations in one ALTER apply in order; track the table as they go.
        let mut table = original.clone();
        let mut table_name = name.to_string();
        let mut changes = Vec::new();
        let push = |change: Change, table: &mut Table, changes: &mut Vec<Change>| {
            let mut scratch = Schema {
                tables: vec![table.clone()],
                ..Schema::default()
            };
            change.apply_to(&mut scratch);
            if let Some(t) = scratch.tables.pop() {
                *table = t;
            }
            changes.push(change);
        };

        for op in ops {
            match op {
                AlterTableOperation::AddColumn {
                    column_def,
                    if_not_exists,
                    column_position,
                    ..
                } => {
                    let (column, extras) = self.column_def(column_def);
                    if table.column(&column.name).is_some() {
                        if *if_not_exists {
                            continue;
                        }
                        return self.unknown(format!(
                            "column `{table_name}.{}` already exists",
                            column.name
                        ));
                    }
                    let position = match column_position {
                        Some(MySQLColumnPosition::First) => 0,
                        Some(MySQLColumnPosition::After(after)) => table
                            .column_position(&ident(after))
                            .map_or(table.columns.len(), |p| p + 1),
                        None => table.columns.len(),
                    };
                    let change = Change::AddColumn {
                        table: table_name.clone(),
                        column,
                        position,
                    };
                    push(change, &mut table, &mut changes);
                    for extra in extras {
                        let change = self.constraint_change(&table, &table_name, extra);
                        push(change, &mut table, &mut changes);
                    }
                }
                AlterTableOperation::DropColumn {
                    column_names,
                    if_exists,
                    ..
                } => {
                    for col in column_names {
                        let col = ident(col);
                        let Some(position) = table.column_position(&col) else {
                            if *if_exists {
                                continue;
                            }
                            return self
                                .unknown(format!("drop of unknown column `{table_name}.{col}`"));
                        };
                        // Indexes and foreign keys on the column go with it.
                        for index in table.indexes.clone() {
                            if index.columns.iter().any(|c| references_column(c, &col)) {
                                push(
                                    Change::DropIndex {
                                        table: table_name.clone(),
                                        index,
                                    },
                                    &mut table,
                                    &mut changes,
                                );
                            }
                        }
                        for fk in table.foreign_keys.clone() {
                            if fk.columns.contains(&col) {
                                push(
                                    Change::DropForeignKey {
                                        table: table_name.clone(),
                                        fk,
                                    },
                                    &mut table,
                                    &mut changes,
                                );
                            }
                        }
                        let column = table.columns[position].clone();
                        push(
                            Change::DropColumn {
                                table: table_name.clone(),
                                column,
                                position,
                            },
                            &mut table,
                            &mut changes,
                        );
                    }
                }
                AlterTableOperation::RenameColumn {
                    old_column_name,
                    new_column_name,
                } => {
                    let from = ident(old_column_name);
                    if table.column(&from).is_none() {
                        return self
                            .unknown(format!("rename of unknown column `{table_name}.{from}`"));
                    }
                    push(
                        Change::RenameColumn {
                            table: table_name.clone(),
                            from,
                            to: ident(new_column_name),
                        },
                        &mut table,
                        &mut changes,
                    );
                }
                AlterTableOperation::RenameTable {
                    table_name: new_name,
                } => {
                    let new_name = match new_name {
                        RenameTableNameKind::As(n) | RenameTableNameKind::To(n) => self.name(n),
                    };
                    // A bare new name keeps the old schema (Postgres `ALTER TABLE s.t RENAME TO u`).
                    let new_name = match table_name.rsplit_once('.') {
                        Some((schema_part, _)) if !new_name.contains('.') => {
                            format!("{schema_part}.{new_name}")
                        }
                        _ => new_name,
                    };
                    changes.push(Change::RenameTable {
                        from: table_name.clone(),
                        to: new_name.clone(),
                    });
                    table.name.clone_from(&new_name);
                    table_name = new_name;
                }
                AlterTableOperation::ChangeColumn {
                    old_name,
                    new_name,
                    data_type,
                    options,
                    ..
                } => {
                    let from = ident(old_name);
                    let Some(before) = table.column(&from).cloned() else {
                        return self
                            .unknown(format!("change of unknown column `{table_name}.{from}`"));
                    };
                    let mut after = Column {
                        name: ident(new_name),
                        data_type: crate::types::canonical_type(
                            &data_type.to_string(),
                            self.dialect,
                        ),
                        nullable: true,
                        ..Column::default()
                    };
                    for option in options {
                        self.column_option(&mut after, option, None);
                    }
                    push(
                        Change::AlterColumn {
                            table: table_name.clone(),
                            before,
                            after,
                        },
                        &mut table,
                        &mut changes,
                    );
                }
                AlterTableOperation::ModifyColumn {
                    col_name,
                    data_type,
                    options,
                    ..
                } => {
                    let name = ident(col_name);
                    let Some(before) = table.column(&name).cloned() else {
                        return self
                            .unknown(format!("modify of unknown column `{table_name}.{name}`"));
                    };
                    let mut after = Column {
                        name,
                        data_type: crate::types::canonical_type(
                            &data_type.to_string(),
                            self.dialect,
                        ),
                        nullable: true,
                        ..Column::default()
                    };
                    for option in options {
                        self.column_option(&mut after, option, None);
                    }
                    push(
                        Change::AlterColumn {
                            table: table_name.clone(),
                            before,
                            after,
                        },
                        &mut table,
                        &mut changes,
                    );
                }
                AlterTableOperation::AlterColumn { column_name, op } => {
                    let name = ident(column_name);
                    let Some(before) = table.column(&name).cloned() else {
                        return self
                            .unknown(format!("alter of unknown column `{table_name}.{name}`"));
                    };
                    let mut after = before.clone();
                    match op {
                        AlterColumnOperation::SetNotNull => after.nullable = false,
                        AlterColumnOperation::DropNotNull => after.nullable = true,
                        AlterColumnOperation::SetDefault { value } => {
                            after.default = Some(value.to_string());
                        }
                        AlterColumnOperation::DropDefault => after.default = None,
                        AlterColumnOperation::SetDataType { data_type, .. } => {
                            after.data_type =
                                crate::types::canonical_type(&data_type.to_string(), self.dialect);
                        }
                        AlterColumnOperation::AddGenerated { .. } => {
                            after.generated = Some(op.to_string());
                        }
                    }
                    push(
                        Change::AlterColumn {
                            table: table_name.clone(),
                            before,
                            after,
                        },
                        &mut table,
                        &mut changes,
                    );
                }
                AlterTableOperation::AddConstraint { constraint, .. } => {
                    let Some(c) = self.table_constraint(constraint) else {
                        return self.unknown(format!("unsupported constraint: {constraint}"));
                    };
                    let change = self.constraint_change(&table, &table_name, c);
                    push(change, &mut table, &mut changes);
                }
                AlterTableOperation::DropConstraint {
                    name, if_exists, ..
                } => {
                    let name = ident(name);
                    match drop_named(&table, &table_name, &name) {
                        Some(change) => push(change, &mut table, &mut changes),
                        None if *if_exists => {}
                        None => {
                            return self.unknown(format!(
                                "drop of unknown constraint `{name}` on `{table_name}`"
                            ));
                        }
                    }
                }
                AlterTableOperation::DropForeignKey { name, .. }
                | AlterTableOperation::DropIndex { name } => {
                    let name = ident(name);
                    match drop_named(&table, &table_name, &name) {
                        Some(change) => push(change, &mut table, &mut changes),
                        None => {
                            return self.unknown(format!(
                                "drop of unknown index/key `{name}` on `{table_name}`"
                            ));
                        }
                    }
                }
                AlterTableOperation::DropPrimaryKey { .. } => push(
                    Change::SetPrimaryKey {
                        table: table_name.clone(),
                        before: table.primary_key.clone(),
                        after: vec![],
                    },
                    &mut table,
                    &mut changes,
                ),
                AlterTableOperation::RenameConstraint { old_name, new_name } => push(
                    Change::RenameConstraint {
                        table: table_name.clone(),
                        from: ident(old_name),
                        to: ident(new_name),
                    },
                    &mut table,
                    &mut changes,
                ),
                AlterTableOperation::EnableRowLevelSecurity
                | AlterTableOperation::ForceRowLevelSecurity => push(
                    Change::SetRowLevelSecurity {
                        table: table_name.clone(),
                        enabled: true,
                    },
                    &mut table,
                    &mut changes,
                ),
                AlterTableOperation::DisableRowLevelSecurity => push(
                    Change::SetRowLevelSecurity {
                        table: table_name.clone(),
                        enabled: false,
                    },
                    &mut table,
                    &mut changes,
                ),
                // How MySQL performs the ALTER, not what it changes.
                AlterTableOperation::Algorithm { .. }
                | AlterTableOperation::Lock { .. }
                | AlterTableOperation::ValidateConstraint { .. } => {}
                other => {
                    changes.push(Change::Unknown {
                        sql: self.sql.to_string(),
                        reason: format!("ALTER TABLE operation not modelled: {other}"),
                    });
                }
            }
        }
        changes
    }

    fn create_index(&self, ci: &CreateIndex) -> Vec<Change> {
        let table_name = self.name(&ci.table_name);
        let Some(table) = self.schema.table(&table_name) else {
            // Indexes on (materialized) views: tracked by name only.
            if self.schema.views.iter().any(|v| v.name == table_name)
                && let Some(name) = &ci.name
            {
                return self.create_object(
                    "index",
                    unqualified(&self.name(name)).to_string(),
                    Some(table_name),
                    ci.if_not_exists,
                );
            }
            return self.unknown(format!("index on unknown table `{table_name}`"));
        };
        let name = ci
            .name
            .as_ref()
            .map(|n| unqualified(&self.name(n)).to_string());
        if let Some(n) = &name
            && table.indexes.iter().any(|i| i.name.as_deref() == Some(n))
        {
            return if ci.if_not_exists {
                vec![]
            } else {
                self.unknown(format!("index `{n}` already exists"))
            };
        }
        let mut index = Index {
            name,
            columns: ci.columns.iter().map(index_column).collect(),
            unique: ci.unique,
            constraint: false,
            using: ci.using.as_ref().map(ToString::to_string),
            predicate: ci.predicate.as_ref().map(ToString::to_string),
        };
        if index.name.is_none() {
            index.name = default_index_name(table, &index, self.dialect);
        }
        vec![Change::AddIndex {
            table: table_name,
            index,
        }]
    }

    fn drop(
        &self,
        kind: ObjectType,
        name: &ObjectName,
        if_exists: bool,
        cascade: bool,
        on_table: Option<&ObjectName>,
    ) -> Vec<Change> {
        let name = self.name(name);
        if kind == ObjectType::Schema && cascade {
            return self.drop_schema_cascade(&name, if_exists);
        }
        match kind {
            ObjectType::Table => match self.schema.table(&name) {
                Some(table) => vec![Change::DropTable {
                    table: table.clone(),
                }],
                None if self
                    .schema
                    .objects
                    .iter()
                    .any(|o| o.kind == "virtual table" && o.name == name) =>
                {
                    self.drop_object("virtual table", &name, if_exists)
                }
                None if if_exists => vec![],
                None => self.unknown(format!("drop of unknown table `{name}`")),
            },
            ObjectType::View | ObjectType::MaterializedView => {
                match self.schema.views.iter().find(|v| v.name == name) {
                    Some(view) => vec![Change::DropView { view: view.clone() }],
                    None if if_exists => vec![],
                    None => self.unknown(format!("drop of unknown view `{name}`")),
                }
            }
            ObjectType::Index => {
                let index_name = unqualified(&name).to_string();
                let table = match on_table {
                    Some(t) => self.schema.table(&self.name(t)),
                    None => self.schema.table_with_index(&index_name),
                };
                let found = table.and_then(|t| {
                    t.indexes
                        .iter()
                        .find(|i| i.name.as_deref() == Some(index_name.as_str()))
                        .map(|i| (t.name.clone(), i.clone()))
                });
                match found {
                    Some((table, index)) => vec![Change::DropIndex { table, index }],
                    None if if_exists => vec![],
                    None => self.unknown(format!("drop of unknown index `{index_name}`")),
                }
            }
            ObjectType::Type => match self.schema.enums.iter().find(|e| e.name == name) {
                Some(e) => vec![Change::DropEnum {
                    enum_type: e.clone(),
                }],
                None => self.drop_object("type", &name, if_exists),
            },
            ObjectType::Schema => self.drop_object("schema", &name, if_exists),
            ObjectType::Sequence => self.drop_object("sequence", &name, if_exists),
            other => self.unknown(format!("DROP {other} is not modelled")),
        }
    }

    /// `DROP SCHEMA s CASCADE`: everything in the schema goes with it.
    fn drop_schema_cascade(&self, schema_name: &str, if_exists: bool) -> Vec<Change> {
        let prefix = format!("{schema_name}.");
        let inside = |name: &str| name.starts_with(&prefix);
        let mut changes: Vec<Change> = Vec::new();
        changes.extend(
            self.schema
                .views
                .iter()
                .filter(|v| inside(&v.name))
                .map(|v| Change::DropView { view: v.clone() }),
        );
        changes.extend(
            self.schema
                .tables
                .iter()
                .filter(|t| inside(&t.name))
                .map(|t| Change::DropTable { table: t.clone() }),
        );
        changes.extend(
            self.schema
                .enums
                .iter()
                .filter(|e| inside(&e.name))
                .map(|e| Change::DropEnum {
                    enum_type: e.clone(),
                }),
        );
        changes.extend(
            self.schema
                .objects
                .iter()
                .filter(|o| inside(&o.name) || o.table.as_deref().is_some_and(inside))
                .map(|o| Change::DropObject { object: o.clone() }),
        );
        changes.extend(self.drop_object("schema", schema_name, if_exists));
        changes
    }

    fn comment(
        &self,
        kind: CommentObject,
        target: &ObjectName,
        comment: Option<String>,
    ) -> Vec<Change> {
        let full = self.name(target);
        let (table, column) = match kind {
            CommentObject::Table => (full, None),
            CommentObject::Column => match full.rsplit_once('.') {
                Some((t, c)) => (t.to_string(), Some(c.to_string())),
                None => return self.unknown("COMMENT ON COLUMN needs table.column"),
            },
            // Comments on other objects don't affect anything rustable tracks.
            _ => {
                return vec![Change::Session {
                    sql: self.sql.to_string(),
                }];
            }
        };
        let Some(t) = self.schema.table(&table) else {
            return self.unknown(format!("comment on unknown table `{table}`"));
        };
        let before = match &column {
            None => t.comment.clone(),
            Some(c) => match t.column(c) {
                Some(col) => col.comment.clone(),
                None => return self.unknown(format!("comment on unknown column `{table}.{c}`")),
            },
        };
        vec![Change::SetComment {
            table,
            column,
            before,
            after: comment,
        }]
    }

    // ---- columns and constraints -------------------------------------------------------------

    /// A column definition, plus any constraints declared inline on it.
    fn column_def(&self, def: &ColumnDef) -> (Column, Vec<Constraint>) {
        let name = ident(&def.name);
        let data_type = crate::types::canonical_type(&def.data_type.to_string(), self.dialect);
        let mut column = Column {
            nullable: !is_serial(&data_type),
            name,
            data_type,
            ..Column::default()
        };
        let mut extras = Vec::new();
        for option in &def.options {
            let constraint_name = option.name.as_ref().map(ident);
            if let Some(extra) = self.column_option(&mut column, &option.option, constraint_name) {
                extras.push(extra);
            }
        }
        (column, extras)
    }

    /// Apply one column option; inline constraints are returned for the caller to attach.
    fn column_option(
        &self,
        column: &mut Column,
        option: &ColumnOption,
        constraint_name: Option<String>,
    ) -> Option<Constraint> {
        let col = column.name.clone();
        match option {
            ColumnOption::Null => column.nullable = true,
            ColumnOption::NotNull => column.nullable = false,
            ColumnOption::Default(e) => column.default = Some(e.to_string()),
            ColumnOption::PrimaryKey(_) => {
                column.nullable = false;
                let text = option.to_string().to_ascii_uppercase();
                if text.contains("AUTOINCREMENT") {
                    column.auto_increment = true;
                }
                return Some(Constraint::PrimaryKey(vec![col]));
            }
            ColumnOption::Unique(_) => {
                return Some(Constraint::Index(Index {
                    name: constraint_name,
                    columns: vec![col],
                    unique: true,
                    constraint: true,
                    ..Index::default()
                }));
            }
            ColumnOption::ForeignKey(fk) => {
                return Some(Constraint::ForeignKey(ForeignKey {
                    name: constraint_name.or_else(|| fk.name.as_ref().map(ident)),
                    columns: vec![col],
                    references_table: self.name(&fk.foreign_table),
                    references_columns: fk.referred_columns.iter().map(ident).collect(),
                    on_delete: fk.on_delete.as_ref().map(ToString::to_string),
                    on_update: fk.on_update.as_ref().map(ToString::to_string),
                }));
            }
            ColumnOption::Check(check) => {
                return Some(Constraint::Check(Check {
                    name: constraint_name.or_else(|| check.name.as_ref().map(ident)),
                    expr: check.expr.to_string(),
                }));
            }
            ColumnOption::DialectSpecific(tokens) => {
                let text: String = tokens
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" ");
                let upper = text.to_ascii_uppercase();
                if upper.contains("AUTO_INCREMENT") || upper.contains("AUTOINCREMENT") {
                    column.auto_increment = true;
                }
            }
            ColumnOption::CharacterSet(n) => column.charset = Some(n.to_string()),
            ColumnOption::Collation(n) => column.collation = Some(n.to_string()),
            ColumnOption::Comment(c) => column.comment = Some(c.clone()),
            ColumnOption::OnUpdate(e) => column.on_update = Some(e.to_string()),
            ColumnOption::Generated { .. } | ColumnOption::Identity(_) => {
                column.generated = Some(option.to_string());
                column.nullable = false;
            }
            _ => {}
        }
        None
    }

    fn table_constraint(&self, constraint: &TableConstraint) -> Option<Constraint> {
        Some(match constraint {
            TableConstraint::PrimaryKey(pk) => {
                Constraint::PrimaryKey(pk.columns.iter().map(index_column).collect())
            }
            TableConstraint::Unique(u) => Constraint::Index(Index {
                name: u.name.as_ref().or(u.index_name.as_ref()).map(ident),
                columns: u.columns.iter().map(index_column).collect(),
                unique: true,
                constraint: true,
                using: u.index_type.as_ref().map(ToString::to_string),
                predicate: None,
            }),
            TableConstraint::Index(i) => Constraint::Index(Index {
                name: i.name.as_ref().map(ident),
                columns: i.columns.iter().map(index_column).collect(),
                unique: false,
                constraint: false,
                using: i.index_type.as_ref().map(ToString::to_string),
                predicate: None,
            }),
            TableConstraint::FulltextOrSpatial(f) => Constraint::Index(Index {
                name: f.opt_index_name.as_ref().map(ident),
                columns: f.columns.iter().map(index_column).collect(),
                unique: false,
                constraint: false,
                using: Some(if f.fulltext {
                    "FULLTEXT".into()
                } else {
                    "SPATIAL".into()
                }),
                predicate: None,
            }),
            TableConstraint::ForeignKey(fk) => Constraint::ForeignKey(ForeignKey {
                name: fk.name.as_ref().or(fk.index_name.as_ref()).map(ident),
                columns: fk.columns.iter().map(ident).collect(),
                references_table: self.name(&fk.foreign_table),
                references_columns: fk.referred_columns.iter().map(ident).collect(),
                on_delete: fk.on_delete.as_ref().map(ToString::to_string),
                on_update: fk.on_update.as_ref().map(ToString::to_string),
            }),
            TableConstraint::Check(c) => Constraint::Check(Check {
                name: c.name.as_ref().map(ident),
                expr: c.expr.to_string(),
            }),
            _ => return None,
        })
    }

    /// The change that adds `constraint` to an existing table (naming it if the database would).
    fn constraint_change(&self, table: &Table, table_name: &str, constraint: Constraint) -> Change {
        let mut scratch = table.clone();
        let before_pk = table.primary_key.clone();
        add_constraint(&mut scratch, constraint.clone());
        fill_default_names(&mut scratch, self.dialect);
        let table_name = table_name.to_string();
        match constraint {
            Constraint::PrimaryKey(cols) => Change::SetPrimaryKey {
                table: table_name,
                before: before_pk,
                after: cols,
            },
            Constraint::Index(_) => Change::AddIndex {
                table: table_name,
                index: scratch.indexes.last().cloned().unwrap_or_default(),
            },
            Constraint::ForeignKey(_) => Change::AddForeignKey {
                table: table_name,
                fk: scratch.foreign_keys.last().cloned().unwrap_or_default(),
            },
            Constraint::Check(_) => Change::AddCheck {
                table: table_name,
                check: scratch.checks.last().cloned().unwrap_or_default(),
            },
        }
    }
}

/// Give unnamed constraints the names the database would generate, so later migrations can
/// drop them by that name (Postgres `users_email_key`, MySQL `orders_ibfk_1`, …).
pub(crate) fn fill_default_names(table: &mut Table, dialect: Dialect) {
    let t = unqualified(&table.name).to_string();
    match dialect {
        Dialect::Postgres => {
            for i in 0..table.indexes.len() {
                if table.indexes[i].name.is_none() && table.indexes[i].constraint {
                    let cols = column_names_for_naming(&table.indexes[i].columns);
                    table.indexes[i].name = Some(format!("{t}_{cols}_key"));
                }
            }
            for fk in &mut table.foreign_keys {
                if fk.name.is_none() {
                    fk.name = Some(format!("{t}_{}_fkey", fk.columns.join("_")));
                }
            }
            let columns: Vec<String> = table.columns.iter().map(|c| c.name.clone()).collect();
            for check in &mut table.checks {
                if check.name.is_none() {
                    let col = first_referenced_column(&check.expr, &columns);
                    check.name = Some(match col {
                        Some(c) => format!("{t}_{c}_check"),
                        None => format!("{t}_check"),
                    });
                }
            }
        }
        Dialect::MySql => {
            for i in 0..table.indexes.len() {
                if table.indexes[i].name.is_none() {
                    let base = table.indexes[i]
                        .columns
                        .first()
                        .map(|c| index_column_name(c))
                        .unwrap_or_default();
                    let mut name = base.clone();
                    let mut n = 2;
                    while table
                        .indexes
                        .iter()
                        .any(|x| x.name.as_deref() == Some(name.as_str()))
                    {
                        name = format!("{base}_{n}");
                        n += 1;
                    }
                    table.indexes[i].name = Some(name);
                }
            }
            let mut n = 1;
            for i in 0..table.foreign_keys.len() {
                if table.foreign_keys[i].name.is_none() {
                    table.foreign_keys[i].name = Some(format!("{t}_ibfk_{n}"));
                    n += 1;
                }
            }
            let mut n = 1;
            for check in &mut table.checks {
                if check.name.is_none() {
                    check.name = Some(format!("{t}_chk_{n}"));
                    n += 1;
                }
            }
        }
        Dialect::Sqlite => {}
    }
}

#[derive(Clone, Debug)]
enum Constraint {
    PrimaryKey(Vec<String>),
    Index(Index),
    ForeignKey(ForeignKey),
    Check(Check),
}

fn add_constraint(table: &mut Table, constraint: Constraint) {
    match constraint {
        Constraint::PrimaryKey(cols) => {
            for c in &mut table.columns {
                if cols.contains(&c.name) {
                    c.nullable = false;
                }
            }
            table.primary_key = cols;
        }
        Constraint::Index(i) => table.indexes.push(i),
        Constraint::ForeignKey(fk) => table.foreign_keys.push(fk),
        Constraint::Check(c) => table.checks.push(c),
    }
}

/// Find a named index / foreign key / check on `table` and return the change dropping it.
fn drop_named(table: &Table, table_name: &str, name: &str) -> Option<Change> {
    let table_name = table_name.to_string();
    if let Some(fk) = table
        .foreign_keys
        .iter()
        .find(|f| f.name.as_deref() == Some(name))
    {
        return Some(Change::DropForeignKey {
            table: table_name,
            fk: fk.clone(),
        });
    }
    if let Some(check) = table
        .checks
        .iter()
        .find(|c| c.name.as_deref() == Some(name))
    {
        return Some(Change::DropCheck {
            table: table_name,
            check: check.clone(),
        });
    }
    if let Some(index) = table
        .indexes
        .iter()
        .find(|i| i.name.as_deref() == Some(name))
    {
        return Some(Change::DropIndex {
            table: table_name,
            index: index.clone(),
        });
    }
    if name.ends_with("_pkey") && !table.primary_key.is_empty() {
        return Some(Change::SetPrimaryKey {
            table: table_name,
            before: table.primary_key.clone(),
            after: vec![],
        });
    }
    None
}

fn index_column(c: &IndexColumn) -> String {
    // Backticks are MySQL-only quoting; drop them so `email`(20) reads as email(20).
    let rendered = c.column.to_string().replace('`', "");
    // Normalise identifier quoting/case for plain column references.
    let (head, tail) = rendered.split_once(' ').unwrap_or((rendered.as_str(), ""));
    let is_plain = head
        .chars()
        .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '`' || ch == '"');
    if is_plain {
        let name = head.trim_matches(|ch| ch == '`' || ch == '"');
        let name = if head.starts_with('`') || head.starts_with('"') {
            name.to_string()
        } else {
            name.to_lowercase()
        };
        if tail.is_empty() {
            name
        } else {
            format!("{name} {tail}")
        }
    } else {
        rendered
    }
}

/// Does a rendered index column (`email`, `lower(email)`, `email(20)`) reference `col`?
fn references_column(rendered: &str, col: &str) -> bool {
    rendered
        .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .any(|word| word == col)
}

/// The column a rendered index column refers to: `email(20)` / `created_at DESC` → the name.
fn index_column_name(c: &str) -> String {
    c.split(['(', ' '])
        .next()
        .unwrap_or(c)
        .trim_matches(|ch| ch == '`' || ch == '"')
        .to_string()
}

fn column_names_for_naming(cols: &[String]) -> String {
    cols.iter()
        .map(|c| {
            let name = index_column_name(c);
            if name.chars().all(|ch| ch.is_alphanumeric() || ch == '_') {
                name
            } else {
                "expr".into()
            }
        })
        .collect::<Vec<_>>()
        .join("_")
}

fn first_referenced_column(expr: &str, columns: &[String]) -> Option<String> {
    expr.split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .map(str::to_lowercase)
        .find(|word| columns.contains(word))
}

pub(crate) fn default_index_name(table: &Table, index: &Index, dialect: Dialect) -> Option<String> {
    match dialect {
        Dialect::Postgres => Some(format!(
            "{}_{}_idx",
            unqualified(&table.name),
            column_names_for_naming(&index.columns)
        )),
        Dialect::MySql => index.columns.first().map(|c| index_column_name(c)),
        Dialect::Sqlite => None,
    }
}

fn is_serial(data_type: &str) -> bool {
    matches!(
        data_type.to_ascii_uppercase().as_str(),
        "SERIAL" | "BIGSERIAL" | "SMALLSERIAL" | "SERIAL4" | "SERIAL8" | "SERIAL2"
    )
}

fn table_options(options: &CreateTableOptions) -> std::collections::BTreeMap<String, String> {
    let list: &[SqlOption] = match options {
        CreateTableOptions::None => &[],
        CreateTableOptions::With(o)
        | CreateTableOptions::Options(o)
        | CreateTableOptions::Plain(o)
        | CreateTableOptions::TableProperties(o) => o,
    };
    list.iter()
        .filter_map(|o| {
            let text = o.to_string();
            let (key, value) = text.split_once('=').unwrap_or((text.as_str(), ""));
            let key = crate::heuristics::normalize_option_key(key.trim());
            // AUTO_INCREMENT=42 is a counter, not part of the schema's shape.
            (key != "AUTO_INCREMENT").then(|| (key, value.trim().trim_matches('\'').to_string()))
        })
        .collect()
}

fn comment_text(rendered: &str) -> String {
    let text = rendered.trim();
    let text = text
        .strip_prefix("COMMENT")
        .map_or(text, |t| t.trim_start().trim_start_matches('=').trim());
    text.trim_matches('\'').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::statements::split_and_parse;

    fn fold(sql: &str, dialect: Dialect) -> (Schema, Vec<Change>) {
        let mut schema = Schema::default();
        let stmts = split_and_parse(sql, dialect);
        let changes = apply_all(&mut schema, &stmts, dialect);
        (schema, changes)
    }

    #[test]
    fn mysql_create_and_alter() {
        let (schema, changes) = fold(
            "CREATE TABLE `users` (
               `id` bigint unsigned NOT NULL AUTO_INCREMENT,
               `email` varchar(320) NOT NULL,
               `status` enum('a','b') NOT NULL DEFAULT 'a',
               PRIMARY KEY (`id`),
               UNIQUE KEY `uq_email` (`email`),
               KEY (`status`)
             ) ENGINE=InnoDB AUTO_INCREMENT=42 DEFAULT CHARSET=utf8mb4;
             ALTER TABLE `users` ADD COLUMN `nick` varchar(64) NULL AFTER `email`, ALGORITHM=INSTANT;
             ALTER TABLE `users` CHANGE COLUMN `nick` `nickname` varchar(32) NOT NULL;
             ALTER TABLE `users` DROP INDEX `status`;",
            Dialect::MySql,
        );
        let users = schema.table("users").unwrap();
        assert_eq!(users.primary_key, vec!["id"]);
        assert!(users.columns[0].auto_increment);
        assert_eq!(
            users
                .columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["id", "email", "nickname", "status"]
        );
        assert!(!users.column("nickname").unwrap().nullable);
        assert_eq!(
            users.options.get("ENGINE").map(String::as_str),
            Some("InnoDB")
        );
        assert!(!users.options.contains_key("AUTO_INCREMENT"));
        assert_eq!(users.indexes.len(), 1, "{:#?}", users.indexes);
        assert!(changes.iter().all(Change::is_schema_change), "{changes:#?}");
    }

    #[test]
    fn postgres_default_constraint_names() {
        let (schema, changes) = fold(
            "CREATE TABLE products (id bigserial PRIMARY KEY, sku text UNIQUE, price_cents int CHECK (price_cents >= 0));
             ALTER TABLE products DROP CONSTRAINT products_price_cents_check;
             ALTER TABLE products DROP CONSTRAINT products_sku_key;",
            Dialect::Postgres,
        );
        let products = schema.table("products").unwrap();
        assert_eq!(products.checks, vec![]);
        assert_eq!(products.indexes, vec![]);
        assert!(!products.column("id").unwrap().nullable);
        assert!(
            !changes.iter().any(|c| matches!(c, Change::Unknown { .. })),
            "{changes:#?}"
        );
    }

    #[test]
    fn renames_follow_through() {
        let (schema, _) = fold(
            "CREATE TABLE a (id int PRIMARY KEY, email text);
             CREATE INDEX a_email_idx ON a (lower(email));
             CREATE TABLE b (id int, a_id int REFERENCES a (id));
             ALTER TABLE a RENAME COLUMN id TO a_pk;
             ALTER TABLE a RENAME TO accounts;",
            Dialect::Postgres,
        );
        let accounts = schema.table("accounts").unwrap();
        assert_eq!(accounts.primary_key, vec!["a_pk"]);
        let fk = &schema.table("b").unwrap().foreign_keys[0];
        assert_eq!(fk.references_table, "accounts");
        assert_eq!(fk.references_columns, vec!["a_pk"]);
    }

    #[test]
    fn unparsed_statements_are_classified() {
        let (_, changes) = fold(
            "PRAGMA foreign_keys = ON; DO $$ BEGIN PERFORM 1; END $$; INSERT INTO t VALUES (1);",
            Dialect::Postgres,
        );
        assert!(matches!(changes[0], Change::Session { .. }), "{changes:#?}");
        assert!(matches!(changes[1], Change::Unknown { .. }));
        assert!(matches!(changes[2], Change::Data { .. }));
    }
}
