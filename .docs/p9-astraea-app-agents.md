# P9 — Astraea App-Scoped Agents, Newsroom Group Chat & Adaptive Editorial UI (App #2)

- **App ID:** `lnc-newsman` (App #2 theo chuẩn dùng chung `lnc-app-agents`, tiếp nối Pilot #1 `lnc-library` và Astraea Round C6)
- **Skill chuẩn liên kết:** `\\10.21.2.2\data\_agentsync\skills\lnc-app-agents\SKILL.md`
- **Hợp đồng dây dẫn:** `protocol-v1` (`v4s` capability envelope, `lnc-app-bridge.v1` Outbound Reverse Relay WSS, Contract C `/api/v1/apps/{app_id}/*` 7-event SSE, Dual-Mode Telegram)
- **Trạng thái:** Wave 2 **ĐÃ KIỂM CHỨNG ĐẠT GATE 4 REPO** (`Astraea` commit `0a55793` trên nhánh `feat/c6-app-agents` + `LNC-Agent-Hub` + `lnc-library` + `lnc-newsman` + E2E loopback WSS trực tiếp). Chưa deploy Portainer hay chạy live acceptance.

---

## 1. Kiến trúc & Ranh giới An toàn trên `lnc-newsman`

1. **Zero-Trust `v4s` & Row-Level `ResourceCtx` Isolation (`src/connector.rs`, `src/mcp.rs`):**
   - `verify_v4s_grant` xác thực phong bì `v4s` (`V4sGrant`, `ResourceCtx`) với `audience == "lnc-newsman"`, `app_id == "lnc-newsman"`, cửa sổ `90s`, nonce dùng một lần, chữ ký HMAC-SHA256 trên chuỗi 9 dòng chuẩn + SHA-256 `canonical_resource_ctx_json`.
   - `AllowedScope` & `mcp::call_tool_direct` ép lọc `ResourceCtx` theo `regions` và `topics` ngay trong truy vấn SQLite của 12 read tools (`regions`, `topics`, `reader_preferences`, `latest_articles`, `article_get`, `documents`, `tax_calendar`, `series`, `observations`, `notes`, `analyses`, `write_log`) và 7 write tools (`note_create`, `note_update`, `note_delete`, `analysis_create`, `analysis_update`, `article_flag`, `brief_propose`); bảo toàn nguyên vẹn `verified` boolean và disclaimer pháp lý/thuế trên `tax_calendar` / `documents`.
   - Tác vụ đề xuất bản tin (`brief_propose` trong `GATED_TOOLS`) thuộc tier `write_gated`, trả `approval_required` (`action_type: "newsman.brief.publish"`) để Astraea ghi nhận vào `/api/v1/approvals` chờ duyệt trước khi phát hành.
2. **SSRF, Secret Hygiene & Coexistence (`src/fetch.rs`, `src/secrets.rs`, `src/agent_bridge.rs`, `src/main.rs`):**
   - Mọi kết nối HTTP/WSS chiều đi (`connect_pinned_ws`, Hub, Astraea, Telegram, Cygnus, remote agent) đi qua `fetch::Fetcher` (DNS + IP pinning + allowlist `APP_AGENTS_ALLOWED_HOSTS`) trước khi mở kết nối WSS `/api/v1/app-bridge/ws` với `APP_ID = "lnc-newsman"`, `BRIDGE_PROTOCOL = "lnc-app-bridge.v1"`.
   - Toàn bộ secret chỉ đi qua `src/secrets.rs` (`LNC_MASTER_KEY`), trả metadata boolean `has_*`, không bao giờ echo/log.
   - Giữ nguyên tuyệt đối `src/telegram.rs` (`alert_telegram_outbox` + `/api/admin/telegram*`) cho cảnh báo khẩn một chiều; tách riêng Dual-Mode Telegram hội thoại dưới `/api/app-agents/telegram/*`.
   - Tuân thủ quy ước placeholder `{id}` / `{slug}` / `{code}` trong `src/main.rs` và đồng bộ đầy đủ với `API_DOCS.md`.
3. **Editorial Newsroom UI (`ui/src/agentTypes.ts`, `useAppAgents.ts`, `AgentsWorkspace.tsx`, `AgentStudio.tsx`, `CompanionDrawer.tsx`, `main.tsx`, `style.css`):**
   - Tái sử dụng nguyên vẹn lớp headless (`useAppAgents`, `useThreadStream`, `useCompanionContext`, CSRF `X-Requested-With: XMLHttpRequest`, giới hạn `16 KiB` UTF-8 `active_view_context`, parser SSE 7 sự kiện).
   - Phối hợp tự nhiên với 6 chế độ Paper/Sepia/Night (`--paper`, `--paper-card`, `--ink`, `--accent`, `--serif`, `--tabbar-h`, `--touch: 44px`), song ngữ VI/EN đồng bộ và nhúng `CompanionDrawer` vào Article Reader / Reports.

---

## 2. Kết quả Kiểm chứng Chéo 4 Repo (Wave 2 Gate — PASS)

- **1-to-1 Cross-Repo Contract Audit:**
  - Đối chiếu 1-to-1 `/api/app-agents/*` giữa `lnc-newsman/src/main.rs` và `ui/src/useAppAgents.ts` + `CompanionDrawer.tsx`: khớp đầy đủ (kể cả `/round-summary`).
  - Đối chiếu `Astraea/crates/astraea-api/src/routers/app_agents.rs` & `app_bridge.rs` với `LNC-Agent-Hub/internal/control/app_connectors_and_astraea.go` (`X-LNC-Delegation`, `X-LNC-App-Id`, `X-Forwarded-Uri`, `X-Request-Id`): khớp 100%, vector chuẩn `v4s` cho cùng chữ ký HMAC-SHA256 `19fe2dab10613c9963247b1fab556161d8d70042f7a790f7172ff08484368dbc`.
  - Xác nhận toàn bộ `reqwest::Client::builder()` trong `lnc-newsman/src/` chỉ nằm trong `src/fetch.rs` (`Fetcher`).
  - Xác nhận `#![forbid(unsafe_code)]` trên toàn bộ crate Rust của `Astraea` (bao gồm `sdk/rust-axum/agent_bridge.rs` và `crates/astraea-api/tests/app_agents_c6.rs`).
- **Astraea (`D:\Python\projects\Astraea`, nhánh `feat/c6-app-agents`, commit `0a55793170b71221a5a3df7ec97a7fc90ec22a0`):**
  - Phạm vi thay đổi Lane A: `28` file (`Cargo.toml`, `Cargo.lock`, `crates/**`, `packages/db/**`, `sdk/**`), `0` chạm `.docs/`, `deploy/`, `.env.example`, `.github/workflows/`, `apps/ui/`, `tests/`.
  - Env vars mới trong Round C6: **0** (tái sử dụng `ASTRAEA_INTERNAL_TOKEN`, `ASTRAEA_SECRETS_KEY`, `LNC_HUB_BASE_URL`, `LNC_HUB_SERVICE_KEY`, `LNC_HUB_DELEGATION_SECRET`, `ASTRAEA_MCP_DELEGATION_SECRET`).
  - `cargo clippy --workspace --all-targets -- -D warnings` -> **PASS (`exit 0`, 0 warnings)**
  - `cargo test -p astraea-crypto-auth -p astraea-db -p astraea-api -p astraea-worker` -> **38 passed; 0 failed; 0 ignored** (gồm `app_agents_c6.rs` `6/6`)
  - `pytest tests/test_c5_deploy.py tests/test_c1_docs.py -q` -> **19 passed in 0.12s**
  - Alembic PostgreSQL 16 (`0009_app_agents_and_bridge`): `upgrade head` -> `downgrade base` -> `upgrade head` -> `alembic check` -> `0009 (head)` -> **PASS**
- **LNC-Agent-Hub (`D:\Python\projects\LNC-Agent-Hub`):**
  - `go test -count=1 ./...` -> **PASS** trên toàn bộ 6 package có test (`bitwarden`, `control`, `db`, `policy`, `secrets`, `server` — `13/13`, 0 failed)
- **lnc-library (`d:\Python\projects\lnc-library`):**
  - `cargo test` -> **198 passed; 0 failed; 0 ignored**
  - `ui`: `npm test` -> **14/14 passed** (`highlight-plan.test.ts` `8/8` + `agentBridge.test.ts` `6/6`); `npm run build` -> **PASS** (`108` modules)
- **lnc-newsman (`d:\Python\projects\lnc-newsman`):**
  - `cargo test` -> **280 passed; 0 failed; 0 ignored** (0 warnings khi test)
  - `ui`: `npm test` -> **6/6 passed** (`agentBridge.test.ts`); `npm run build` -> **PASS** (`38` modules)
- **4-Repo Live Loopback E2E (`astraea-api` + `lnc-hub` + `lnc-library` + `lnc-newsman` trên PostgreSQL 16 + Mock LLM):**
  - **PASS (`exit 0`):** cả `lnc-library` và `lnc-newsman` mở Outbound Reverse Relay WSS (`GET /api/v1/app-bridge/ws`), gửi `hello` (`lnc-app-bridge.v1`), báo `mode == "astraea_relay"` và `relay_connected == true`.
  - Cross-app isolation trả `403` trên cả Hub proxy và Astraea.
  - Row-level `v4s` `ResourceCtx` isolation chạy qua WSS thật: `lnc-library` chỉ thấy sách thuộc Shelf A (ẩn sách Shelf B); `lnc-newsman` (`regions: ["vn"], topics: ["finance"]`) giữ bài VN và ẩn bài US.
  - Quan sát đầy đủ 7 loại SSE event (`seat_state`, `reasoning`, `tool`, `tool_result`, `text`, `result`, `error`).
  - Cả hai write tool nhạy cảm (`library.audiobook.create` và `newsman.brief.publish`) trả `approval_required` thay vì tự thực thi, và `Astraea` ghi nhận bản ghi trong `/api/v1/approvals`.
