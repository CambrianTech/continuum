//! Compile declared cascade closure into a database-side recursive identity relation.
//! No record body or materialized victim list crosses the storage boundary.
use super::{
    adapter::naming,
    query::StorageQuery,
    types::{CascadeRule, CollectionSchema, FieldType},
};
use sea_orm::sea_query::{
    Alias, CommonTableExpression, Expr, JoinType, Query, UnionType, WithClause,
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) const VICTIMS: &str = "__continuum_delete_closure";

impl super::relational::RelationalStore {
    pub(super) fn deletion_graph(
        &self,
        schema: &CollectionSchema,
        query: &StorageQuery,
    ) -> Result<(WithClause, Vec<String>), String> {
        let schemas = self
            .collections()
            .iter()
            .map(|name| self.schema(name))
            .collect::<Result<Vec<_>, _>>()?;
        let names: BTreeMap<_, _> = schemas
            .iter()
            .map(|declared| {
                (
                    naming::to_table_name(&declared.collection),
                    declared.collection.clone(),
                )
            })
            .collect();
        let mut reachable = BTreeSet::from([naming::to_table_name(&schema.collection)]);
        loop {
            let before = reachable.len();
            for child in &schemas {
                if child.fields.iter().any(|field| {
                    field.foreign_key.as_ref().is_some_and(|reference| {
                        reference.on_delete == CascadeRule::Cascade
                            && reachable.contains(&naming::to_table_name(&reference.collection))
                    })
                }) {
                    reachable.insert(naming::to_table_name(&child.collection));
                }
            }
            if reachable.len() == before {
                break;
            }
        }
        // Qualify physical tables so a user's table name cannot shadow the CTE.
        let physical = |name: &str| (Alias::new("main"), Alias::new(naming::to_table_name(name)));
        let mut closure = Query::select();
        closure
            .expr(Expr::val(schema.collection.clone()))
            .column(Alias::new("id"))
            .from(physical(&schema.collection))
            .cond_where(super::relational_query::condition(schema, query)?);
        for child in &schemas {
            if !reachable.contains(&naming::to_table_name(&child.collection)) {
                continue;
            }
            for field in &child.fields {
                let Some(reference) = &field.foreign_key else {
                    continue;
                };
                if reference.on_delete != CascadeRule::Cascade
                    || !reachable.contains(&naming::to_table_name(&reference.collection))
                {
                    continue;
                }
                let parent_name = names
                    .get(&naming::to_table_name(&reference.collection))
                    .ok_or("cascade parent has no admitted declaration")?;
                let c = Alias::new("child_row");
                let p = Alias::new("parent_row");
                let v = Alias::new("victim_row");
                let branch = Query::select()
                    .expr(Expr::val(child.collection.clone()))
                    .column((c.clone(), Alias::new("id")))
                    .from_as(physical(&child.collection), c.clone())
                    .join_as(
                        JoinType::InnerJoin,
                        physical(&reference.collection),
                        p.clone(),
                        Expr::col((c, Alias::new(naming::to_snake_case(&field.name)))).eq(
                            Expr::col((
                                p.clone(),
                                Alias::new(naming::to_snake_case(&reference.field)),
                            )),
                        ),
                    )
                    .join_as(
                        JoinType::InnerJoin,
                        Alias::new(VICTIMS),
                        v.clone(),
                        Expr::col((v.clone(), Alias::new("collection")))
                            .eq(parent_name.clone())
                            .and(
                                Expr::col((v, Alias::new("id")))
                                    .eq(Expr::col((p, Alias::new("id")))),
                            ),
                    )
                    .to_owned();
                // UNION, not UNION ALL: cycles and converging cascade paths visit
                // each (collection,id) once, with no arbitrary traversal depth cap.
                closure.union(UnionType::Distinct, branch);
            }
        }
        let mut cte = CommonTableExpression::new();
        cte.table_name(Alias::new(VICTIMS))
            .column(Alias::new("collection"))
            .column(Alias::new("id"))
            .query(closure);
        let mut with = WithClause::new();
        with.recursive(true).cte(cte);
        let native = schemas
            .iter()
            .filter(|declared| {
                reachable.contains(&naming::to_table_name(&declared.collection))
                    && declared
                        .fields
                        .iter()
                        .any(|field| field.field_type == FieldType::Binary)
            })
            .map(|declared| declared.collection.clone())
            .collect();
        Ok((with, native))
    }
}
#[cfg(test)]
#[path = "relational_delete_graph_tests.rs"]
mod tests;
