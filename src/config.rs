use anyhow::{bail, Result};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::discovery::discover;
use crate::paths::{project_path, ProjectPath};
use crate::util::{new_uuid_string, uuid_hex};
use crate::{
    DEFAULT_COLLECTION, DEFAULT_DIM, DEFAULT_ENDPOINT, DEFAULT_MODEL, DEFAULT_PROVIDER,
    MAX_CONTENT_BYTES, SCHEMA_VERSION,
};

include!("config/types.rs");
include!("config/io.rs");
include!("config/defaults.rs");
include!("config/tests.rs");
