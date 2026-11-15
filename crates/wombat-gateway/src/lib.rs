use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    path::{Path as FsPath, PathBuf},
    sync::Arc,
    time::Duration,
};

use axum::{
    extract::{Path, State},
    http::{header::HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, SecondsFormat, Utc};
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
    sync::RwLock,
    time,
};
use tower_http::cors::{Any, CorsLayer};
use tracing::{debug, warn};

const DEFAULT_ALLOWED_ORIGIN: &str = "http://localhost:3000";
const DEFAULT_BIND: &str = "127.0.0.1:3000";
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(250);
const MAX_EVENT_LINE_BYTES: usize = 1024 * 1024;
const FILE_READ_CHUNK_BYTES: usize = 64 * 1024;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedEvent {
    pub wonb_id: String,
    pub aoi_id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub topic: String,
    pub issued_at: String,
    pub simulation: bool,
    pub data_origin: Option<String>,
    pub verified: bool,
    pub payload: Value,
    pub provenance: Value,
}

#[derive(Debug, Deserialize)]
struct EventInput {
    wonb_id: String,
    aoi_id: String,
    #[serde(rename = "type", default)]
    type_: Option<String>,
    #[serde(default)]
    wonb_type: Option<String>,
    topic: String,
    issued_at: String,
    simulation: bool,
    #[serde(default)]
    data_origin: Option<String>,
    verified: bool,
    payload: Value,
    provenance: Value,
}

impl TryFrom<EventInput> for VerifiedEvent {
    type Error = &'static str;

    fn try_from(event: EventInput) -> Result<Self, Self::Error> {
        let type_ = event.type_.or(event.wonb_type).ok_or("type is missing")?;
        if !event.verified {
            return Err("verified is false");
        }
        if event.wonb_id.is_empty()
            || event.aoi_id.is_empty()
            || type_.is_empty()
            || event.topic.is_empty()
            || event.issued_at.is_empty()
        {
            return Err("required event field is empty");
        }
        Ok(Self {
            wonb_id: event.wonb_id,
            aoi_id: event.aoi_id,
            type_,
            topic: event.topic,
            issued_at: event.issued_at,
            simulation: event.simulation,
            data_origin: event.data_origin,
            verified: event.verified,
            payload: event.payload,
            provenance: event.provenance,
        })
    }
}

#[derive(Clone, Default)]
pub struct EventStore {
    events_by_aoi: Arc<RwLock<HashMap<String, VerifiedEvent>>>,
    seen_ids: Arc<RwLock<HashSet<String>>>,
}

impl EventStore {
    pub async fn insert_verified(&self, event: VerifiedEvent) -> bool {
        let mut seen_ids = self.seen_ids.write().await;
        if !seen_ids.insert(event.wonb_id.clone()) {
            return false;
        }
        drop(seen_ids);

        let mut events = self.events_by_aoi.write().await;
        let replace = events
            .get(&event.aoi_id)
            .is_none_or(|current| event_order(&event) > event_order(current));
        if replace {
            events.insert(event.aoi_id.clone(), event);
        }
        true
    }

    pub async fn latest(&self) -> Option<VerifiedEvent> {
        self.events_by_aoi
            .read()
            .await
            .values()
            .max_by(|left, right| event_order(left).cmp(&event_order(right)))
            .cloned()
    }

    pub async fn latest_for_aoi(&self, aoi_id: &str) -> Option<VerifiedEvent> {
        self.events_by_aoi.read().await.get(aoi_id).cloned()
    }
}

fn event_order(event: &VerifiedEvent) -> (&str, &str) {
    (&event.issued_at, &event.wonb_id)
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

pub fn allowed_origin_from_env() -> Result<AllowedOrigin, GatewayConfigError> {
    let origin = std::env::var("WOMBAT_GATEWAY_ALLOWED_ORIGIN")
        .unwrap_or_else(|_| DEFAULT_ALLOWED_ORIGIN.to_owned());
    parse_allowed_origin(&origin)
}

#[derive(Debug, Clone)]
pub enum AllowedOrigin {
    Any,
    Exact(HeaderValue),
}

pub fn parse_allowed_origin(origin: &str) -> Result<AllowedOrigin, GatewayConfigError> {
    if origin == "*" {
        return Ok(AllowedOrigin::Any);
    }
    HeaderValue::from_str(origin)
        .map(AllowedOrigin::Exact)
        .map_err(|error| GatewayConfigError(format!("origen CORS inválido {origin:?}: {error}")))
}

#[derive(Clone)]
struct GatewayState {
    risks: RiskStore,
    events: EventStore,
}

pub fn router(store: RiskStore, events: EventStore, allowed_origin: AllowedOrigin) -> Router {
    let cors = match allowed_origin {
        AllowedOrigin::Any => CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([Method::GET]),
        AllowedOrigin::Exact(origin) => CorsLayer::new()
            .allow_origin(origin)
            .allow_methods([Method::GET]),
    };

    Router::new()
        .route("/health", get(health))
        .route("/api/v1/events/latest", get(latest_event))
        .route("/api/v1/events/{aoi_id}", get(event_for_aoi))
        .route("/api/v1/risks/latest", get(latest_risk))
        .route("/api/v1/risks/{aoi_id}", get(risk_for_aoi))
        .with_state(GatewayState {
            risks: store,
            events,
        })
        .layer(cors)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn latest_risk(State(state): State<GatewayState>) -> Result<Json<RiskResponse>, ApiError> {
    state
        .risks
        .latest()
        .await
        .map(Json)
        .ok_or_else(ApiError::no_risks)
}

async fn risk_for_aoi(
    State(state): State<GatewayState>,
    Path(aoi_id): Path<String>,
) -> Result<Json<RiskResponse>, ApiError> {
    state
        .risks
        .latest_for_aoi(&aoi_id)
        .await
        .map(Json)
        .ok_or_else(|| ApiError::aoi_not_found(&aoi_id))
}

async fn latest_event(State(state): State<GatewayState>) -> Result<Json<VerifiedEvent>, ApiError> {
    state
        .events
        .latest()
        .await
        .map(Json)
        .ok_or_else(ApiError::no_events)
}

async fn event_for_aoi(
    State(state): State<GatewayState>,
    Path(aoi_id): Path<String>,
) -> Result<Json<VerifiedEvent>, ApiError> {
    state
        .events
        .latest_for_aoi(&aoi_id)
        .await
        .map(Json)
        .ok_or_else(|| ApiError::no_events_for_aoi(&aoi_id))
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
    fn no_events() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "no_verified_events",
            message: "Aún no hay eventos WONB verificados disponibles.".into(),
        }
    }

    fn no_events_for_aoi(aoi_id: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "no_verified_events",
            message: format!("No hay eventos verificados para el AOI {aoi_id:?}."),
        }
    }

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
        .unwrap_or_else(|_| DEFAULT_BIND.to_owned())
        .parse()
}

pub fn event_input_from_env() -> Option<PathBuf> {
    std::env::var_os("WOMBAT_GATEWAY_EVENT_IN").map(PathBuf::from)
}

#[derive(Debug, Parser)]
#[command(name = "wombat-gateway", about = "API HTTP de solo lectura WOMBAT")]
pub struct GatewayArgs {
    #[arg(long, env = "WOMBAT_GATEWAY_BIND", default_value = DEFAULT_BIND)]
    pub bind: SocketAddr,
    #[arg(long, env = "WOMBAT_GATEWAY_EVENT_IN")]
    pub event_in: Option<PathBuf>,
}

#[derive(Default)]
struct TailState {
    offset: u64,
    pending: Vec<u8>,
    checkpoint: Vec<u8>,
    discarding_line: bool,
    #[cfg(unix)]
    identity: Option<(u64, u64)>,
}

pub async fn follow_event_file(path: PathBuf, store: EventStore) {
    let mut tail = TailState::default();
    let mut interval = time::interval(EVENT_POLL_INTERVAL);
    loop {
        interval.tick().await;
        if let Err(error) = read_new_events(&path, &store, &mut tail).await {
            debug!(file = %path.display(), %error, "No se pudo leer archivo JSONL; se reintentará");
        }
    }
}

async fn read_new_events(
    path: &FsPath,
    store: &EventStore,
    tail: &mut TailState,
) -> std::io::Result<()> {
    let mut file = match File::open(path).await {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let metadata = file.metadata().await?;
    let len = metadata.len();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let identity = (metadata.dev(), metadata.ino());
        if tail.identity.is_some_and(|previous| previous != identity) {
            tail.offset = 0;
            tail.pending.clear();
            tail.checkpoint.clear();
            tail.discarding_line = false;
        }
        tail.identity = Some(identity);
    }
    if len < tail.offset {
        tail.offset = 0;
        tail.pending.clear();
        tail.checkpoint.clear();
        tail.discarding_line = false;
    } else if tail.offset > 0 && !tail.checkpoint.is_empty() {
        let checkpoint_start = tail.offset.saturating_sub(tail.checkpoint.len() as u64);
        file.seek(std::io::SeekFrom::Start(checkpoint_start))
            .await?;
        let mut current_checkpoint = vec![0; tail.checkpoint.len()];
        file.read_exact(&mut current_checkpoint).await?;
        if current_checkpoint != tail.checkpoint {
            tail.offset = 0;
            tail.pending.clear();
            tail.checkpoint.clear();
            tail.discarding_line = false;
        }
    }
    file.seek(std::io::SeekFrom::Start(tail.offset)).await?;
    let mut chunk = [0; FILE_READ_CHUNK_BYTES];
    loop {
        let count = file.read(&mut chunk).await?;
        if count == 0 {
            break;
        }
        tail.offset = tail.offset.saturating_add(count as u64);
        tail.pending.extend_from_slice(&chunk[..count]);
        ingest_complete_lines(&mut tail.pending, &mut tail.discarding_line, store).await;
    }
    let checkpoint_len = tail.offset.min(64) as usize;
    tail.checkpoint = vec![0; checkpoint_len];
    if checkpoint_len > 0 {
        file.seek(std::io::SeekFrom::Start(
            tail.offset.saturating_sub(checkpoint_len as u64),
        ))
        .await?;
        file.read_exact(&mut tail.checkpoint).await?;
    }
    Ok(())
}

async fn ingest_complete_lines(
    pending: &mut Vec<u8>,
    discarding_line: &mut bool,
    store: &EventStore,
) {
    while let Some(newline) = pending.iter().position(|byte| *byte == b'\n') {
        let mut line: Vec<u8> = pending.drain(..=newline).collect();
        line.pop();
        if *discarding_line {
            *discarding_line = false;
            continue;
        }
        if line.len() > MAX_EVENT_LINE_BYTES {
            warn!(
                max_bytes = MAX_EVENT_LINE_BYTES,
                "Línea JSONL excede el tamaño máximo; se ignoró"
            );
            continue;
        }
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if !line.is_empty() {
            ingest_event_line(&line, store).await;
        }
    }
    if pending.len() > MAX_EVENT_LINE_BYTES {
        pending.clear();
        *discarding_line = true;
        warn!(
            max_bytes = MAX_EVENT_LINE_BYTES,
            "Línea JSONL excede el tamaño máximo; se descartará hasta el siguiente newline"
        );
    }
}

async fn ingest_event_line(line: &[u8], store: &EventStore) {
    let parsed = serde_json::from_slice::<EventInput>(line)
        .map_err(|error| format!("JSONL inválido: {error}"))
        .and_then(|input| VerifiedEvent::try_from(input).map_err(str::to_owned));
    match parsed {
        Ok(event) => {
            if !store.insert_verified(event).await {
                debug!("Evento verificado duplicado ignorado");
            }
        }
        Err(error) => warn!(%error, "Línea JSONL ignorada"),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

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
            EventStore::default(),
            parse_allowed_origin("http://localhost:3000").unwrap(),
        )
    }

    fn event(aoi_id: &str, wonb_id: &str, issued_at: &str) -> Value {
        json!({
            "event_type": "verified_wonb",
            "wonb_id": wonb_id,
            "aoi_id": aoi_id,
            "type": "change",
            "wonb_type": "change",
            "topic": format!("wombat/demo/{aoi_id}/change"),
            "issued_at": issued_at,
            "simulation": true,
            "data_origin": "simulated",
            "verified": true,
            "payload": {"message": "DEMO SIMULADA — no es alerta científica"},
            "provenance": {"sources": []}
        })
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
    async fn verified_jsonl_event_is_available_from_latest_endpoint() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("events.jsonl");
        let line =
            serde_json::to_vec(&event("piura", "wonb-piura-1", "2026-11-15T10:00:00Z")).unwrap();
        std::fs::write(&input, [line.as_slice(), b"\n"].concat()).unwrap();

        let store = EventStore::default();
        let mut tail = TailState::default();
        read_new_events(&input, &store, &mut tail).await.unwrap();
        let api = router(
            RiskStore::default(),
            store,
            parse_allowed_origin("http://localhost:3000").unwrap(),
        );
        let (status, body) = get_json(api, "/api/v1/events/latest").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["wonb_id"], "wonb-piura-1");
        assert_eq!(body["aoi_id"], "piura");
        assert_eq!(body["type"], "change");
        assert_eq!(body["topic"], "wombat/demo/piura/change");
        assert_eq!(body["issued_at"], "2026-11-15T10:00:00Z");
        assert_eq!(body["simulation"], true);
        assert_eq!(body["data_origin"], "simulated");
        assert_eq!(body["verified"], true);
        assert_eq!(
            body["payload"]["message"],
            "DEMO SIMULADA — no es alerta científica"
        );
        assert!(body["provenance"].is_object());
    }

    #[tokio::test]
    async fn missing_event_file_is_retried_when_it_appears() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("events.jsonl");
        let store = EventStore::default();
        let mut tail = TailState::default();
        read_new_events(&input, &store, &mut tail).await.unwrap();
        assert!(store.latest().await.is_none());

        std::fs::write(
            &input,
            format!(
                "{}\n",
                serde_json::to_string(&event("piura", "appeared", "2026-11-15T10:00:00Z")).unwrap()
            ),
        )
        .unwrap();
        read_new_events(&input, &store, &mut tail).await.unwrap();
        assert_eq!(store.latest().await.unwrap().wonb_id, "appeared");
    }

    #[tokio::test]
    async fn unverified_and_invalid_jsonl_lines_are_ignored_without_stopping_ingestion() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("events.jsonl");
        let mut unverified = event("piura", "unverified", "2026-11-15T11:00:00Z");
        unverified["verified"] = json!(false);
        let valid = event("piura", "valid", "2026-11-15T10:00:00Z");
        let contents = format!(
            "{}\n{{broken json\n{}\n",
            serde_json::to_string(&unverified).unwrap(),
            serde_json::to_string(&valid).unwrap()
        );
        std::fs::write(&input, contents).unwrap();

        let store = EventStore::default();
        let mut tail = TailState::default();
        read_new_events(&input, &store, &mut tail).await.unwrap();
        assert_eq!(store.latest().await.unwrap().wonb_id, "valid");
    }

    #[tokio::test]
    async fn events_are_deduplicated_and_kept_separate_by_aoi() {
        let store = EventStore::default();
        let piura: VerifiedEvent =
            serde_json::from_value(event("piura", "same-id", "2026-11-15T10:00:00Z")).unwrap();
        assert!(store.insert_verified(piura.clone()).await);
        assert!(!store.insert_verified(piura).await);
        let thailand: VerifiedEvent =
            serde_json::from_value(event("thailand", "thai-id", "2026-11-15T11:00:00Z")).unwrap();
        assert!(store.insert_verified(thailand).await);

        let api = router(
            RiskStore::default(),
            store,
            parse_allowed_origin("http://localhost:3000").unwrap(),
        );
        let (status, piura_response) = get_json(api.clone(), "/api/v1/events/piura").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(piura_response["wonb_id"], "same-id");
        assert_eq!(piura_response["aoi_id"], "piura");
        let (status, thailand_response) = get_json(api, "/api/v1/events/thailand").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(thailand_response["wonb_id"], "thai-id");
        assert_eq!(thailand_response["aoi_id"], "thailand");
    }

    #[tokio::test]
    async fn incomplete_jsonl_line_is_buffered_until_completed() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("events.jsonl");
        let serialized =
            serde_json::to_vec(&event("piura", "partial", "2026-11-15T10:00:00Z")).unwrap();
        let split = serialized.len() / 2;
        std::fs::write(&input, &serialized[..split]).unwrap();
        let store = EventStore::default();
        let mut tail = TailState::default();
        read_new_events(&input, &store, &mut tail).await.unwrap();
        assert!(store.latest().await.is_none());

        let mut remainder = serialized[split..].to_vec();
        remainder.push(b'\n');
        std::fs::OpenOptions::new()
            .append(true)
            .open(&input)
            .unwrap()
            .write_all(&remainder)
            .unwrap();
        read_new_events(&input, &store, &mut tail).await.unwrap();
        assert_eq!(store.latest().await.unwrap().wonb_id, "partial");
    }

    #[tokio::test]
    async fn truncated_and_rewritten_file_is_read_from_the_beginning() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("events.jsonl");
        let first =
            serde_json::to_string(&event("piura", "before-truncate", "2026-11-15T10:00:00Z"))
                .unwrap();
        std::fs::write(&input, format!("{first}\n")).unwrap();
        let store = EventStore::default();
        let mut tail = TailState::default();
        read_new_events(&input, &store, &mut tail).await.unwrap();

        let mut replacement = event("piura", "after-truncate", "2026-11-15T11:00:00Z");
        replacement["payload"]["padding"] = json!("x".repeat(first.len()));
        std::fs::write(
            &input,
            format!("{}\n", serde_json::to_string(&replacement).unwrap()),
        )
        .unwrap();
        read_new_events(&input, &store, &mut tail).await.unwrap();

        assert_eq!(store.latest().await.unwrap().wonb_id, "after-truncate");
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

    #[tokio::test]
    async fn cors_supports_configured_origin_and_explicit_wildcard() {
        for (configured, expected) in [
            ("http://dashboard.example", "http://dashboard.example"),
            ("*", "*"),
        ] {
            let app = router(
                RiskStore::default(),
                EventStore::default(),
                parse_allowed_origin(configured).unwrap(),
            );
            let response = app
                .oneshot(
                    Request::builder()
                        .method(Method::OPTIONS)
                        .uri("/api/v1/events/latest")
                        .header("origin", "http://dashboard.example")
                        .header("access-control-request-method", "GET")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response
                    .headers()
                    .get("access-control-allow-origin")
                    .unwrap(),
                expected
            );
        }
    }

    #[test]
    fn cli_has_requested_bind_default_and_accepts_event_input() {
        let defaults = <GatewayArgs as clap::Parser>::try_parse_from(["wombat-gateway"]).unwrap();
        assert_eq!(defaults.bind, "127.0.0.1:3000".parse().unwrap());
        assert!(defaults.event_in.is_none());

        let configured = <GatewayArgs as clap::Parser>::try_parse_from([
            "wombat-gateway",
            "--bind",
            "0.0.0.0:4000",
            "--event-in",
            "/var/lib/wombat/events.jsonl",
        ])
        .unwrap();
        assert_eq!(configured.bind, "0.0.0.0:4000".parse().unwrap());
        assert_eq!(
            configured.event_in,
            Some(PathBuf::from("/var/lib/wombat/events.jsonl"))
        );
    }
}
