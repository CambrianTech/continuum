use super::*;
use sea_orm::sea_query::{ColumnDef, Query, Value};
use sea_orm::{ConnectionTrait, DbBackend, Schema};

async fn fixture(path: &Path, values: Vec<Value>) -> Result<(), String> {
    let db = Database::connect(format!(
        "sqlite://{}?mode=rwc",
        path.to_string_lossy().replace('\\', "/")
    ))
    .await
    .map_err(|error| error.to_string())?;
    let mut table = Schema::new(DbBackend::Sqlite).create_table_from_entity(Entity);
    table.col(ColumnDef::new(Alias::new("payload")).binary());
    db.execute(DbBackend::Sqlite.build(&table))
        .await
        .map_err(|error| error.to_string())?;
    for (index, value) in values.into_iter().enumerate() {
        let insert = Query::insert()
            .into_table(Entity)
            .columns([
                Alias::new("id"),
                Alias::new("learned"),
                Alias::new("payload"),
            ])
            .values([
                format!("{index}").into(),
                value.into(),
                vec![0_u8, 255, 128].into(),
            ])
            .map_err(|error| error.to_string())?
            .to_owned();
        db.execute(DbBackend::Sqlite.build(&insert))
            .await
            .map_err(|error| error.to_string())?;
    }
    db.close().await.map_err(|error| error.to_string())
}

#[tokio::test]
async fn dry_run_apply_reopen_and_repeat_preserve_exact_values_and_bytes() -> Result<(), String> {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let path = directory.path().join("training.db");
    fixture(
        &path,
        vec![
            Value::BigInt(None),
            0_i64.into(),
            i64::from(u32::MAX).into(),
        ],
    )
    .await?;
    assert_eq!(
        run(&path, false).await?,
        Receipt {
            rows: 3,
            historical_nulls: 1,
            changed: 0,
            applied: false
        }
    );
    assert_eq!(
        run(&path, true).await?,
        Receipt {
            rows: 3,
            historical_nulls: 1,
            changed: 1,
            applied: true
        }
    );
    assert_eq!(run(&path, true).await?.changed, 0);
    let db = Database::connect(format!(
        "sqlite://{}?mode=ro",
        path.to_string_lossy().replace('\\', "/")
    ))
    .await
    .map_err(|error| error.to_string())?;
    for (id, expected) in [("0", 0_i64), ("1", 0), ("2", i64::from(u32::MAX))] {
        let query = Query::select()
            .from(Entity)
            .columns([Alias::new("learned"), Alias::new("payload")])
            .and_where(Expr::col(Column::Id).eq(id))
            .to_owned();
        let row = db
            .query_one(DbBackend::Sqlite.build(&query))
            .await
            .map_err(|error| error.to_string())?
            .ok_or("lost row")?;
        assert_eq!(
            row.try_get::<i64>("", "learned")
                .map_err(|error| error.to_string())?,
            expected
        );
        assert_eq!(
            row.try_get::<Vec<u8>>("", "payload")
                .map_err(|error| error.to_string())?,
            vec![0, 255, 128]
        );
    }
    db.close().await.map_err(|error| error.to_string())
}

#[tokio::test]
async fn invalid_progress_rolls_back_without_replacing_unknown_nulls() -> Result<(), String> {
    for invalid in [
        (-1_i64).into(),
        (i64::from(u32::MAX) + 1).into(),
        1.5_f64.into(),
        "invalid".into(),
    ] {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let path = directory.path().join("invalid.db");
        fixture(&path, vec![Value::BigInt(None), invalid]).await?;
        assert!(run(&path, true).await.is_err());
        let db = Database::connect(format!(
            "sqlite://{}?mode=ro",
            path.to_string_lossy().replace('\\', "/")
        ))
        .await
        .map_err(|error| error.to_string())?;
        assert_eq!(
            Entity::find()
                .filter(Column::Learned.is_null())
                .count(&db)
                .await
                .map_err(|error| error.to_string())?,
            1
        );
        db.close().await.map_err(|error| error.to_string())?;
    }
    Ok(())
}
