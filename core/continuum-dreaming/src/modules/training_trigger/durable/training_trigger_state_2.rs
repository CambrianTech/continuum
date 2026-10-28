        }
        migrate_base_change_buckets(self.as_ref()).await?;
        Ok(())
    }