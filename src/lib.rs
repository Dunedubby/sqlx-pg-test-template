//!
//! To find failed database test, you can use the following SQL:
//!
//! ```sql
//! SELECT
//!     d.datname AS database_name,
//!     sd.description AS comment
//! FROM
//!     pg_database d
//! JOIN
//!     pg_shdescription sd ON d.oid = sd.objoid
//! WHERE
//!     sd.description ILIKE '%test%';
//! ```
pub use macros::test;

#[doc(hidden)]
pub use run_test::TestArgs;

#[doc(hidden)]
pub use run_test::run_test;
