use std::sync::Arc;
use alloy_primitives::{Address, FixedBytes, Uint, Signed};
use std::collections::{HashSet, HashMap};
use rocksdb::{DB, ColumnFamily, Options, ColumnFamilyDescriptor};

// database table 이름 타입으로 정의 
pub enum TableName {
    DolomiteBorrowPositions,
    AaveExecuteBorrow,
}

impl TableName {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "dolomite_borrow_positions" => Some(TableName::DolomiteBorrowPositions),
            "aave_execute_borrow" => Some(TableName::AaveExecuteBorrow),
            _ => None
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TableName::DolomiteBorrowPositions => "dolomite_borrow_positions",
            TableName::AaveExecuteBorrow => "aave_execute_borrow",
        }
    }
}

#[derive(Clone)]
pub struct RocksDB {
    db: Arc<DB>
}

impl RocksDB {
    pub fn init(file_path: &str, required_cfs: &[&str]) -> Self {
        let existing_cfs = DB::list_cf(&Options::default(), file_path)
            .unwrap_or_else(|_| vec!["default".to_string()]);
        let existing_cf_set: HashSet<String> = existing_cfs.iter().cloned().collect();

        let cf_descriptors: Vec<_> = existing_cfs
            .iter()
            .map(|cf_name| ColumnFamilyDescriptor::new(cf_name, Options::default()))
            .collect();

        let mut db = DB::open_cf_descriptors(&Options::default(), file_path, cf_descriptors)
            .expect("Failed to open DB");

        for cf in required_cfs {
            if !existing_cf_set.contains(*cf) {
                db.create_cf(*cf, &Options::default())
                    .expect(&format!("Failed to create CF: {}", cf));
            }
        }

        RocksDB {
            db: Arc::new(db)
        }
    }

    pub fn save(&self, cf: &str, k: &str, v: &str) -> bool {
        let cf = self.db.cf_handle(cf).expect("missing CF");
        self.db.put_cf(cf, k.as_bytes(), v.as_bytes()).is_ok()
    }

    pub fn find(&self, cf: &str, k: &str) -> Option<String> {
        let cf = self.db.cf_handle(cf).expect("missing CF");
        match self.db.get_cf(cf, k.as_bytes()) {
            Ok(Some(v)) => {
                let result = String::from_utf8(v).unwrap();
                println!("Finding '{}' returns '{}'", k, result);
                Some(result)
            },
            Ok(None) => {
                println!("Finding '{}' returns None", k);
                None
            },
            Err(e) => {
                println!("Error retrieving value for {}: {}", k, e);
                None
            }
        }
    }

    pub fn delete(&self, cf: &str, k: &str) -> bool {
        let cf = self.db.cf_handle(cf).expect("missing CF");
        self.db.delete_cf(cf, k.as_bytes()).is_ok()
    }
}