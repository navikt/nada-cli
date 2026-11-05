#[allow(unused)]
mod postgres_nais_io;
#[allow(unused)]
mod postgresaccesses_nais_io;
#[allow(unused)]
mod postgresbindings_nais_io;
#[allow(unused)]
mod postgresbranches_nais_io;
#[allow(unused)]
mod postgresinstances_nais_io;

#[allow(unused)]
pub mod crd {
    pub use super::postgres_nais_io::*;
    pub use super::postgresaccesses_nais_io::*;
    pub use super::postgresbindings_nais_io::*;
    pub use super::postgresbranches_nais_io::*;
    pub use super::postgresinstances_nais_io::*;
}
