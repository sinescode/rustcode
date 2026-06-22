//! CORS configuration for the HTTP server.
//!
//! Ported from: `packages/blazecode/src/server/routes/instance/httpapi/middleware/cors-vary.ts`
//! and `packages/blazecode/src/server/routes/instance/httpapi/server.ts` (lines 112–119).

use std::time::Duration;
use tower_http::cors::{Any, CorsLayer};

/// Create the CORS middleware layer.
///
/// # Source
/// Ported from `packages/blazecode/src/server/routes/instance/httpapi/server.ts` lines 112–119:
/// ```ts
/// const cors = (corsOptions?: CorsOptions) =>
///   HttpRouter.middleware(
///     HttpMiddleware.cors({
///       allowedOrigins: (origin) => isAllowedCorsOrigin(origin, corsOptions),
///       maxAge: 86_400,
///     }),
///     { global: true },
///   )
/// ```
///
/// The TS source uses `isAllowedCorsOrigin` to check against a `CorsConfig`
/// provider. In Rust we accept the allowed origins list directly. An empty slice
/// means all origins are allowed (matching the "no CorsOptions" case).
pub fn cors_layer(allowed_origins: &[String]) -> CorsLayer {
    if allowed_origins.is_empty() {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
            .max_age(tower_http::cors::MaxAge::exact(Duration::from_secs(86_400)))
    } else {
        let origins = allowed_origins
            .iter()
            .map(|o| o.parse().expect("invalid CORS origin"))
            .collect::<Vec<_>>();

        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods(Any)
            .allow_headers(Any)
            .max_age(tower_http::cors::MaxAge::exact(Duration::from_secs(86_400)))
    }
}
