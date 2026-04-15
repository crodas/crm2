use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Monetary amount stored as cents (i64) but serialized as float in JSON.
/// e.g. 1500 cents → 15.00 in JSON
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Amount(pub i64);

impl Amount {
    pub fn from_float(f: f64) -> Self {
        Amount((f * 100.0).round() as i64)
    }

    pub fn to_float(self) -> f64 {
        self.0 as f64 / 100.0
    }

    pub fn cents(self) -> i64 {
        self.0
    }
}

impl Serialize for Amount {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(self.to_float())
    }
}

impl<'de> Deserialize<'de> for Amount {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let f = f64::deserialize(deserializer)?;
        Ok(Amount::from_float(f))
    }
}

// sqlx: decode from i64 column
impl sqlx::Type<sqlx::Sqlite> for Amount {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <i64 as sqlx::Type<sqlx::Sqlite>>::type_info()
    }

    fn compatible(ty: &sqlx::sqlite::SqliteTypeInfo) -> bool {
        <i64 as sqlx::Type<sqlx::Sqlite>>::compatible(ty)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for Amount {
    fn decode(value: sqlx::sqlite::SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let cents = <i64 as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
        Ok(Amount(cents))
    }
}

impl sqlx::Encode<'_, sqlx::Sqlite> for Amount {
    fn encode_by_ref(
        &self,
        buf: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'_>>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <i64 as sqlx::Encode<sqlx::Sqlite>>::encode_by_ref(&self.0, buf)
    }
}

impl Amount {
    /// Multiply amount by a quantity (f64), rounding to nearest cent
    pub fn mul_qty(self, qty: f64) -> Amount {
        Amount((self.0 as f64 * qty).round() as i64)
    }
}

impl std::ops::Add for Amount {
    type Output = Amount;
    fn add(self, rhs: Self) -> Self {
        Amount(self.0 + rhs.0)
    }
}

impl std::ops::Sub for Amount {
    type Output = Amount;
    fn sub(self, rhs: Self) -> Self {
        Amount(self.0 - rhs.0)
    }
}

impl std::iter::Sum for Amount {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        Amount(iter.map(|a| a.0).sum())
    }
}
