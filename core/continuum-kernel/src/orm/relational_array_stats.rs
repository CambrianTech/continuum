//! Numeric-array summaries stay in the database; no embedding payload is hydrated.
use super::{adapter::{naming, NumericArrayStats}, types::FieldType};
use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm::sea_query::{Alias, CaseStatement, Condition, Expr, Func, Query, SimpleExpr};

impl super::relational::RelationalStore {
    pub async fn numeric_array_stats(&self, collection: &str, field: &str) -> Result<NumericArrayStats, String> {
        let schema = self.schema(collection)?;
        let declaration = schema.fields.iter().find(|f| f.name == field).ok_or("array statistics require a declared field")?;
        if declaration.field_type != FieldType::Json { return Err("array statistics require a JSON field".into()); }
        let column = super::relational_query::column(&schema, field)?;
        let value = Expr::col((Alias::new("records"), column));
        let length = Func::cust(Alias::new("json_array_length")).arg(value.clone());
        let bad_element = Query::select().expr(Expr::val(1))
            .from_function(Func::cust(Alias::new("json_each")).arg(value.clone()), Alias::new("element"))
            .and_where(Expr::col((Alias::new("element"), Alias::new("type"))).is_not_in(["integer", "real"]))
            .to_owned();
        let invalid = Condition::any()
            .add(Condition::all().add(value.clone().is_not_null())
                .add(Expr::expr(Func::cust(Alias::new("json_type")).arg(value.clone())).ne("array")))
            .add(Expr::exists(bad_element));
        let positive = Expr::expr(length.clone()).gt(0);
        let counted: SimpleExpr = CaseStatement::new().case(positive.clone(), 1).finally(0).into();
        let dimension: SimpleExpr = CaseStatement::new().case(positive, length).finally(Expr::val(Option::<i64>::None)).into();
        let invalid: SimpleExpr = CaseStatement::new().case(invalid, 1).finally(0).into();
        let query = Query::select().from_as(Alias::new(naming::to_table_name(collection)), Alias::new("records"))
            .expr_as(Expr::col((Alias::new("records"), Alias::new("id"))).count(), Alias::new("total"))
            .expr_as(Expr::expr(counted).sum(), Alias::new("nonempty"))
            .expr_as(Expr::expr(dimension.clone()).min(), Alias::new("minimum"))
            .expr_as(Expr::expr(dimension).max(), Alias::new("maximum"))
            .expr_as(Expr::expr(invalid).sum(), Alias::new("invalid"))
            .to_owned();
        let row = self.db.query_one(DbBackend::Sqlite.build(&query)).await.map_err(|e| e.to_string())?.ok_or("array aggregate returned no row")?;
        let number = |name: &str| -> Result<usize, String> {
            let value: Option<i64> = row.try_get("", name).map_err(|e| e.to_string())?;
            usize::try_from(value.unwrap_or(0)).map_err(|_| format!("array statistic {name} exceeds usize"))
        };
        if number("invalid")? != 0 { return Err(format!("{collection}.{field} contains a non-numeric array value")); }
        Ok(NumericArrayStats { total_records: number("total")?, non_empty_records: number("nonempty")?, min_length: number("minimum")?, max_length: number("maximum")? })
    }
}
