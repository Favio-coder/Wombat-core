use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use axum::{
    extract::{Path, State},
    http::{header::HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;

const DEFAULT_ALLOWED_ORIGIN: &str = "http://localhost:3000";

/// Risk ya validado por el nodo/contrato aguas arriba.
///
/// La construcción de este valor es la frontera de confianza del gateway:
/// quien lo entrega debe haber validado tipo, firma, emisor, TTL y cadena.
/// El gateway no vuelve a validar firmas ni calcula rankings.
#[derive(Debug, Clone)]
pub struct VerifiedRisk {
    pub aoi_id: String,
    pub wonb_id: String,
    pub issued_at: DateTime<Utc>,
    pub status: String,
    pub rank: Option<u32>,
    pub score_bp: Option<u32>,
    pub verification_source: String,
    pub payload: Value,
}

/// Representación JSON pública que consume el dashboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskResponse {
    pub aoi_id: String,
    pub wonb_id: String,
    pub issued_at: String,
    pub status: String,
    pub rank: Option<u32>,
    pub score_bp: Option<u32>,
    pub verification: VerificationInfo,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationInfo {
    pub status: VerificationStatus,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Verified,
}

impl From<VerifiedRisk> for RiskResponse {
    fn from(risk: VerifiedRisk) -> Self {
        Self {
            aoi_id: risk.aoi_id,
            wonb_id: risk.wonb_id,
            issued_at: risk.issued_at.to_rfc3339_opts(SecondsFormat::AutoSi, true),
            status: risk.status,
            rank: risk.rank,
            score_bp: risk.score_bp,
            verification: VerificationInfo {
                status: VerificationStatus::Verified,
                source: risk.verification_source,
            },
            payload: risk.payload,
        }
    }
}

#[derive(Clone, Default)]
pub struct RiskStore {
    risks_by_aoi: Arc<RwLock<HashMap<String, RiskResponse>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreOutcome {
    Inserted,
    Replaced,
    KeptNewer,
}

impl RiskStore {
    /// Guarda o reemplaza el último Risk verificado para su AOI.
    ///
    /// Un bloque más antiguo que el ya almacenado no puede hacer retroceder
    /// la caché, aunque llegue después por la red.
    pub async fn store_verified_risk(&self, risk: VerifiedRisk) -> StoreOutcome {
        let response = RiskResponse::from(risk);
        let mut risks = self.risks_by_aoi.write().await;
        match risks.get(&response.aoi_id) {
            Some(existing) if existing.issued_at > response.issued_at => StoreOutcome::KeptNewer,
            Some(_) => {
                risks.insert(response.aoi_id.clone(), response);
                StoreOutcome::Replaced
            }
            None => {
                risks.insert(response.aoi_id.clone(), response);
                StoreOutcome::Inserted
            }
        }
    }

    pub async fn latest(&self) -> Option<RiskResponse> {
        self.risks_by_aoi
            .read()
            .await
            .values()
            .max_by(|left, right| {
                left.issued_at
                    .cmp(&right.issued_at)
                    .then_with(|| left.wonb_id.cmp(&right.wonb_id))
            })
            .cloned()
    }

    pub async fn latest_for_aoi(&self, aoi_id: &str) -> Option<RiskResponse> {
        self.risks_by_aoi.read().await.get(aoi_id).cloned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayConfigError(String);

impl std::fmt::Display for GatewayConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for GatewayConfigError {}

pub fn allowed_origin_from_env() -> Result<HeaderValue, GatewayConfigError> {
    let origin = std::env::var("WOMBAT_GATEWAY_ALLOWED_ORIGIN")
        .unwrap_or_else(|_| DEFAULT_ALLOWED_ORIGIN.to_owned());
    parse_allowed_origin(&origin)
}

pub fn parse_allowed_origin(origin: &str) -> Result<HeaderValue, GatewayConfigError> {
    if origin == "*" {
        return Err(GatewayConfigError(
            "WOMBAT_GATEWAY_ALLOWED_ORIGIN no puede ser '*'".into(),
        ));
    }
    HeaderValue::from_str(origin)
        .map_err(|error| GatewayConfigError(format!("origen CORS inválido {origin:?}: {error}")))
}

pub fn router(store: RiskStore, allowed_origin: HeaderValue) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(allowed_origin)
        .allow_methods([Method::GET]);

    Router::new()
        .route("/health", get(health))
        .route("/api/v1/risks/latest", get(latest_risk))
        .route("/api/v1/risks/{aoi_id}", get(risk_for_aoi))
        .with_state(store)
        .layer(cors)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn latest_risk(State(store): State<RiskStore>) -> Result<Json<RiskResponse>, ApiError> {
    store
        .latest()
        .await
        .map(Json)
        .ok_or_else(ApiError::no_risks)
}

async fn risk_for_aoi(
    State(store): State<RiskStore>,
    Path(aoi_id): Path<String>,
) -> Result<Json<RiskResponse>, ApiError> {
    store
        .latest_for_aoi(&aoi_id)
        .await
        .map(Json)
        .ok_or_else(|| ApiError::aoi_not_found(&aoi_id))
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn no_risks() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "no_verified_risks",
            message: "Aún no hay Risks verificados disponibles.".into(),
        }
    }

    fn aoi_not_found(aoi_id: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "risk_not_found",
            message: format!("No hay un Risk verificado para el AOI {aoi_id:?}."),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.code,
                message: self.message,
            }),
        )
            .into_response()
    }
}

#[derive(Serialize)]
struct ErrorResponse {
    error: &'static str,
    message: String,
}

pub fn bind_address_from_env() -> Result<SocketAddr, std::net::AddrParseError> {
    std::env::var("WOMBAT_GATEWAY_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_owned())
        .parse()
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use chrono::TimeZone;
    use serde_json::{json, Value};
    use tower::ServiceExt;

    use super::*;

    fn app(store: RiskStore) -> Router {
        router(
            store,
            parse_allowed_origin("http://localhost:3000").unwrap(),
        )
    }

    fn risk(aoi_id: &str, wonb_id: &str, issued_at: DateTime<Utc>) -> VerifiedRisk {
        VerifiedRisk {
            aoi_id: aoi_id.into(),
            wonb_id: wonb_id.into(),
            issued_at,
            status: "review_high".into(),
            rank: Some(1),
            score_bp: Some(9_200),
            verification_source: "wombat-node".into(),
            payload: json!({
                "cell_id": format!("{aoi_id}-r00c00"),
                "status": "review_high",
                "rank": 1,
                "score_bp": 9200,
                "explanation": ["demo payload for unit test"]
            }),
        }
    }

    async fn get_json(app: Router, uri: &str) -> (StatusCode, Value) {
        let response = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let (status, body) = get_json(app(RiskStore::default()), "/health").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, json!({"status": "ok"}));
    }

    #[tokio::test]
    async fn empty_latest_and_aoi_endpoints_return_clear_404() {
        let app = app(RiskStore::default());
        let (status, body) = get_json(app.clone(), "/api/v1/risks/latest").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "no_verified_risks");
        let (status, body) = get_json(app, "/api/v1/risks/piura").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "risk_not_found");
    }

    #[tokio::test]
    async fn stored_risk_is_returned_by_global_and_aoi_endpoints() {
        let store = RiskStore::default();
        store
            .store_verified_risk(risk(
                "piura",
                "sha256:risk-1",
                Utc.with_ymd_and_hms(2026, 11, 15, 10, 0, 0).unwrap(),
            ))
            .await;
        let app = app(store);

        for uri in ["/api/v1/risks/latest", "/api/v1/risks/piura"] {
            let (status, body) = get_json(app.clone(), uri).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body["aoi_id"], "piura");
            assert_eq!(body["wonb_id"], "sha256:risk-1");
            assert_eq!(body["verification"]["status"], "verified");
            assert_eq!(body["payload"]["score_bp"], 9200);
        }
    }

    #[tokio::test]
    async fn separate_aois_keep_their_own_latest_risk_and_global_latest_is_newest() {
        let store = RiskStore::default();
        store
            .store_verified_risk(risk(
                "piura",
                "sha256:piura-1",
                Utc.with_ymd_and_hms(2026, 11, 15, 10, 0, 0).unwrap(),
            ))
            .await;
        store
            .store_verified_risk(risk(
                "tumbes",
                "sha256:tumbes-1",
                Utc.with_ymd_and_hms(2026, 11, 15, 11, 0, 0).unwrap(),
            ))
            .await;

        let app = app(store);
        let (status, latest) = get_json(app.clone(), "/api/v1/risks/latest").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(latest["aoi_id"], "tumbes");
        let (status, piura) = get_json(app.clone(), "/api/v1/risks/piura").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(piura["wonb_id"], "sha256:piura-1");
        let (status, tumbes) = get_json(app, "/api/v1/risks/tumbes").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(tumbes["wonb_id"], "sha256:tumbes-1");
    }

    #[tokio::test]
    async fn newer_risk_replaces_previous_for_same_aoi_but_older_does_not() {
        let store = RiskStore::default();
        store
            .store_verified_risk(risk(
                "piura",
                "sha256:risk-1",
                Utc.with_ymd_and_hms(2026, 11, 15, 10, 0, 0).unwrap(),
            ))
            .await;
        assert_eq!(
            store
                .store_verified_risk(risk(
                    "piura",
                    "sha256:risk-2",
                    Utc.with_ymd_and_hms(2026, 11, 15, 11, 0, 0).unwrap(),
                ))
                .await,
            StoreOutcome::Replaced
        );
        assert_eq!(
            store
                .store_verified_risk(risk(
                    "piura",
                    "sha256:risk-old",
                    Utc.with_ymd_and_hms(2026, 11, 15, 9, 0, 0).unwrap(),
                ))
                .await,
            StoreOutcome::KeptNewer
        );
        assert_eq!(
            store.latest_for_aoi("piura").await.unwrap().wonb_id,
            "sha256:risk-2"
        );
    }

    #[test]
    fn wildcard_cors_origin_is_rejected() {
        assert!(parse_allowed_origin("*").is_err());
    }
}
