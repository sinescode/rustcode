//! # opencode-provider
//!
//! Provider-agnostic AI integration layer.
//!
//! ## Architecture
//!
//! ```text
//!         ┌───────────────────┐
//!         │    ProviderPool    │  ← Connection pool, rate limiting, circuit breaker
//!         └────────┬──────────┘
//!                   │
//!         ┌────────▼──────────┐
//!         │   ProviderClient   │  ← Typed API (stream, complete, embed)
//!         └────────┬──────────┘
//!                   │
//!         ┌────────▼──────────┐
//!         │  ProviderAdapter   │  ← Anthropic / OpenAI / Google / ...
//!         └────────┬──────────┘
//!                   │
//!         ┌────────▼──────────┐
//!         │    ProviderRouter  │  ← Model selection, fallback, load balancing
//!         └───────────────────┘
//! ```

#![deny(unsafe_code)]
#![deny(missing_docs)]

pub mod client;
pub mod adapter;
pub mod cache;
pub mod router;
pub mod types;

pub use client::*;
pub use types::*;
pub use cache::*;
pub use router::*;
