        }
        migrate_base_change_buckets(self).await?;
        Ok(())
    }


    /// Two indexed metadata pages