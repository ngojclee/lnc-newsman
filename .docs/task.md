# LNC Newsman — Task

Cập nhật: 12/11/2026 (v0.4.2 — P8 nguồn văn bản pháp luật có cổng điều khoản; P7 Telegram)

## Đã xong

- [x] Đọc các skill chuẩn cho blueprint app, agent và Portainer; khảo sát style, route Agent Hub và sổ cổng.
- [x] Chốt phạm vi đa lĩnh vực, kênh cảnh báo, VI/EN và mô hình nhiều người đọc, một admin.
- [x] Viết `.docs/plan.md`; anh duyệt dựng P0 bằng Rust + React ngày 11/11/2026.
- [x] Tạo agent `news-analyst-argus` (`.md` + `.toml`) trong `F:\_agentsync\agents`; kiểm tra cặp file và contract store.

## P0 — Khung app

- [x] Khung Rust/Axum, auth, health, settings hiển thị, API đọc và SQLite migrations với chủ đề mẫu.
- [x] UI React/Vite: `/admin`, VI/EN, Giấy/Sepia/Đêm và điều hướng responsive.
- [x] Dự thảo Dockerfile, stack image-pull, env example và workflow build thủ công; **chưa xác thực container**.
- [x] Kiểm thử local; bộ hiện tại có 23 Rust tests, format/clippy, UI build và browser smoke desktop/mobile.
- [ ] Build image và chạy container/healthcheck: Docker/Podman không có trên PATH máy đang kiểm tra.
- [ ] Hoàn thiện mô hình cấu hình ba tầng, kho secret mã hóa và các kiểm soát cần thiết trước production.
- [ ] Hoàn tất quy trình icon `lnc-icon`: generator canonical, raster và kiểm tra 32px; hiện mới có SVG dự thảo.
- [ ] Chấm cổng chất lượng plan khi Business MCP Hub hết lỗi phiên.
- [ ] Chỉ giữ cổng sau khi được duyệt và xác minh; `8462` hiện mới là đề xuất.

## P1 — Nguồn tin và thu thập

- [x] Sổ nguồn và cây chủ đề: thêm, sửa, bật/tắt; kiểm tra trùng, vòng lặp và phụ thuộc; UI song ngữ.
- [x] Migration SQLite catalog có test giữ dữ liệu cũ; browser smoke sử dụng database kiểm thử riêng.
- [x] Bộ tải RSS/Atom an toàn: chặn địa chỉ nội bộ sau khi phân giải DNS, ghim IP, kiểm tra từng bước chuyển hướng, giới hạn 5 MB và thời gian chờ.
- [x] Đọc RSS 2.0/RDF/Atom; chỉ lưu tiêu đề, link, mã định danh và ngày đăng; khử trùng lặp; ETag/Last-Modified.
- [x] Lịch quét có công tắc chung (mặc định tắt), giãn cách theo máy chủ, lùi lịch khi lỗi, tự tạm dừng sau 5 lần lỗi; nút Quét ngay, Tiếp tục, Kiểm tra feed.
- [x] Trang Hôm nay và Chủ đề hiển thị tin thật từ kho; API `GET /api/v1/articles`.
- [x] Dropdown tự vẽ bo góc đồng bộ; đổi font serif có đủ glyph tiếng Việt; sửa màu nhấn chế độ Đêm.
- [ ] Nghiệm thu P1 với ít nhất 20 nguồn RSS thật — **cần anh duyệt việc tải feed thật và danh sách nguồn**.
- [ ] Bậc thang fetch phần còn lại: HTML trực tiếp + robots.txt, `lnc-scrapling`, proxy, agent; hỗ trợ feed không phải UTF-8; dọn bài cũ.

## P1.5 — Cây vùng và region pack (kế hoạch v0.2)

- [x] Sửa mục 4.10: `global` là gốc ảo (chính thư mục gốc của pack), không phải thư mục anh em; lồng thư mục = cha/con.
- [x] Migration 5: bảng `regions` có vùng cha, gắn `region` cho nguồn, `region_code` cho bài, `pack_key` cho nguồn do pack quản lý; có test giữ dữ liệu P0/P1 (nguồn/bài cũ vào `global`).
- [x] Region pack trong `REGIONS_DIR` (mặc định `<DATA_DIR>/regions`); pack mặc định global/eu/eu-fr/us/vn nhúng trong binary, ghi ra khi thư mục trống.
- [x] Quét khi khởi động và nút **Nạp lại vùng**: chỉ đăng ký, mọi vùng (trừ `global`) và nguồn mới ở trạng thái **tắt**; kiểm tra cú pháp, mã trùng/lệch, cha thiếu, chu trình, URL, trùng nguồn thủ công, symlink/path escape; giữ cấu hình hợp lệ lần trước.
- [x] Tập vùng hiệu lực = vùng bật + mọi vùng cha; scheduler, `/api/articles`, `/api/v1/articles` (`region`) dùng tập này; thêm `/api/v1/regions`.
- [x] Bộ chọn vùng trên thanh trên (dùng `Select.tsx`); trang Cài đặt có cây vùng bật/tắt, nút Nạp lại, danh sách lỗi pack; bảng nguồn hiện nhãn "do region pack quản lý".
- [x] Migration: giá trị `sources.region` chữ tự do từ P1 (vd "France") được đưa về `global` để bài cũ không biến mất; mỗi lần nạp pack lặp lại fallback cho dòng bị sửa tay và gán lại `region_code` của bài theo nguồn.
- [x] Số liệu `/api/dashboard` lọc theo cùng tập vùng hiệu lực (và theo `?region`), để thẻ "tin" không lớn hơn danh sách thực tế.
- [x] Test: 39 Rust tests (gồm cây/kế thừa, FR gồm EU, thư mục mới nhận nhưng tắt, các loại pack lỗi, giữ cấu hình khi nạp lỗi, symlink, migration, quyền admin, scheduler bỏ qua vùng tắt) — tất cả dùng loopback/máy giả lập.
- [x] Cảnh báo trong bảng nguồn khi nguồn đang bật nhưng vùng đang tắt (trước đây scheduler im lặng bỏ qua).
- [x] Smoke trình duyệt local (loopback, key demo, DB tạm): bật eu → fr, chọn Pháp, pack lỗi hiện trong UI, nguồn pack chỉ bật/tắt, nguồn tay bị từ chối khi vùng tắt; Giấy/Sepia/Đêm và 375 px không tràn ngang.
- [ ] Nghiệm thu P1.5 với danh sách nguồn thật theo vùng — **cần anh duyệt tải nguồn thật** (dự án chưa có nguồn thật nào).
- [ ] Official/tax/markets/macro trong pack: để P2–P4 như đã thống nhất.

## P2 — Nền tảng (không gọi nhà cung cấp thật)

- [x] Kho secret AEAD: XChaCha20-Poly1305 dưới `LNC_MASTER_KEY`, nonce mới mỗi lần ghi, tên khóa là AAD; `LNC_MASTER_KEY_OLD` + `POST /api/admin/secrets/rewrap` hai pha (không dòng nào bị ghi nếu có dòng không mở được); chưa có khóa ⇒ 503; API/UI chỉ trả metadata + fingerprint.
- [x] Schema `series`/`observations`/`series_events`/`connectors`: `observed_at` (kỳ), `as_of` (thời điểm biết), `revision`, chất lượng, nguồn, đơn vị; sửa = thêm revision; truy vấn as-of.
- [x] Khung connector: trait + context chứa `fetch::Fetcher` của hệ thống; cổng enabled → vùng → secret → hạn mức/ngày → cache → lịch do khung áp; ghi health/last_error/hạn mức mỗi lần thử.
- [x] Connector giả lập loopback (`#[cfg(test)]`) để test khung.
- [x] API đọc `/api/v1/series`, `/api/v1/series/{code}/observations?as_of=`, `/api/v1/events`; UI Thị trường + khối Khóa nhà cung cấp/Connector.
- [x] Cảnh báo an toàn A: validator chỉ gửi cho đúng origin, xóa validator khi đổi URL; giãn cách host áp cho cả Quét ngay/Kiểm tra feed; kết quả bị hủy nếu URL/vùng đổi giữa chừng; parser bắt buộc thấy thẻ gốc feed; test hủy giữa chừng nhả in-flight.
- [x] Connector thật theo bộ nguồn anh chọn (`.docs/sources-candidates.md` mục 6): mempool.space, ECB SDMX, Federal Register, FRED (khóa), CoinGecko Demo (khóa, ≤2/phút, ghi nguồn bắt buộc), SEC EDGAR (`SEC_USER_AGENT` bắt buộc). Mỗi cái có test loopback riêng; hạn mức đếm theo request; attribution theo điều khoản.
- [x] Pack `vn`: nguồn VnExpress ở trạng thái tắt, ghi chú điều khoản ngay trong file; không thêm URL chưa xác nhận (VnEconomy, Thanh Niên).
- [x] Lưu văn bản chính thức: bảng `documents` metadata + API `/api/v1/documents`; không lưu `abstract` của nhà cung cấp.
- [x] Kịch bản nghiệm thu trực tiếp `.docs/live-acceptance.md` — **chưa chạy, chờ anh duyệt**.
- [ ] Chạy nghiệm thu trực tiếp từng nguồn theo kịch bản và ghi kết quả vào PROJECT_STATUS.
- [ ] Lưu filing SEC (EDGAR hiện chỉ đếm entry), biểu đồ, lịch chạy tự động cho connector.
- [ ] Légifrance/PISTE, chứng khoán VN/Pháp, ETF Farside — chưa đủ điều khoản/nguồn.
- [ ] Lịch chạy tự động cho connector, biểu đồ, và nghiệm thu P2.

## P3 — Nền tảng văn bản chính thức & thuế (offline, 11/11/2026)

- [x] Pack `official.toml` (cơ quan ban hành) và `tax.toml` (nguồn thuế + hạn nộp) cho global/eu/eu-fr/us/vn; kiểm tra từng file; mọi mục mặc định TẮT.
- [x] `seed_builtin` bổ sung file mới cho bản cài đã có mà **không ghi đè** file chủ dự án đã sửa (lỗi cũ: chỉ seed khi chưa có `region.toml`).
- [x] `documents`: thêm `topic`, `disclaimer`; FTS5 bỏ dấu tiếng Việt (`thue` → `thuế`); token được quote nên query không chèn được cú pháp FTS.
- [x] Tóm tắt tự viết luôn mang disclaimer do server đặt; tài liệu chưa có tóm tắt thì không gắn disclaimer.
- [x] Engine lịch thuế: cadence tháng/quý/năm, múi giờ vùng (fallback UTC), quy tắc cuối tuần theo khai báo, kẹp ngày 29–31 vào cuối tháng ngắn, `due_soon` theo `lead_days`.
- [x] API `/api/v1/tax-calendar`, `/api/v1/documents?q=&topic=`; admin `GET/PUT /api/admin/tax-rules`.
- [x] UI trang **Vùng** (tin + văn bản + lịch thuế), Paper/Sepia/Night, 375 px.
- [ ] **Xác minh hạn nộp thật theo luật từng vùng** — mọi rule trong pack là mẫu (`verified = false`), cần chủ dự án đối chiếu với cơ quan thuế rồi sửa và đặt `verified = true`.
- [ ] Connector văn bản chính thức theo vùng (EUR-Lex/Légifrance/IRS…) và nhắc hạn qua Telegram/email (P7).

## P5 — Nền tảng phân loại (offline, 11/11/2026)

- [x] Phân loại bằng luật: chủ đề theo từ khóa (thứ tự khai báo giữ tie), vùng tinh từ tên địa danh khi nguồn là `global`, thực thể theo mẫu (TICKER, EXCHANGE:SYMBOL, CRYPTO, CURRENCY, ORG).
- [x] Lưu nhãn vào `article_matches` kèm nguồn gốc (`rule`/`llm`/`human`); nhãn do người sửa **không bị luật ghi đè**, và nhãn luật cũ cùng loại bị xoá để không hiện hai chủ đề mâu thuẫn.
- [x] Gắn nhãn luật ngay khi thu thập bài mới (trong cùng transaction); lỗi gắn nhãn không làm mất bài.
- [x] Lớp LLM: mặc định **tắt**, cần `LLM_BASE_URL` + khóa `llm-api-key`; gọi qua `fetch::Fetcher` (SSRF), token budget/ngày kiểm **trước** khi gọi, cache theo hash(model+prompt+title), lỗi/không đủ hạn mức ⇒ giữ nhãn luật.
- [x] API `GET/PATCH /api/admin/llm`, `POST /api/admin/classify`, `POST /api/admin/classify/reclassify`.
- [x] Đo trên 50 mục mẫu tách dev/holdout: **dev 96%, holdout 84%** (ngưỡng 80%).
- [x] Sửa lỗi deadlock thật: `status()` giữ khoá DB rồi gọi `enabled()` khoá lần nữa — nay đọc switch trong cùng khoá.
- [ ] Đo với LLM thật (cần chủ dự án chọn Agent Hub hay LiteLLM + ngân sách token).
- [ ] Tóm tắt bằng LLM và gom story (phần còn lại của P5).
- [x] Đưa nhãn lên người đọc: `/api/articles` trả `labels {topics, entities}` (một truy vấn mỗi trang) và UI hiện chip chủ đề + thực thể (11/11/2026).

## P4 nền tảng filings + P5/P6 provider Hub (11/11/2026, offline)

- [x] Tra điều khoản EDGAR/AMF/ESMA/HOSE và ghi mức A/B/C vào `.docs/sources-candidates.md` mục 7 (chỉ EDGAR mức A; AMF B/C; ESAP chưa hoạt động; VN C).
- [x] Migration 14: `companies`, `filings`, `financial_facts` (as-of + revision); chỉ metadata + số liệu + link, không toàn văn (có test).
- [x] Connector `sec-edgar-filings` (mặc định TẮT, `SEC_USER_AGENT` bắt buộc, 2 request/công ty, test loopback); sửa lỗi thật: thiếu lớp `filings.recent` và thiếu `rename_all = camelCase`.
- [x] API `/api/v1/companies`, `/{id}/filings`, `/{id}/facts`; disclaimer "không phải tư vấn đầu tư" tự động.
- [x] `src/hub.rs`: LLM OpenAI-compatible theo alias tác vụ + agent run + khóa riêng trong secret + allowlist host nội bộ theo tên + token budget + cache + timeout + kiểm tra kết nối (chỉ liệt kê model).
- [x] UI Cài đặt: khối LNC Agent Hub (khóa chỉ ghi, nút Kiểm tra kết nối, hiện host/alias/hạn mức nhưng không hiện khóa/URL đầy đủ).
- [x] `.docs/hub-integration.md`: việc phía Hub admin (khóa, quyền, alias), biến môi trường, cách kiểm, ranh giới dữ liệu.
- [ ] **Chưa gọi Hub thật**; chưa biết Hub có đúng route `/chat/completions`, `/models`, `/agents/{id}/run`.
- [ ] Connector AMF/ESEF và Việt Nam (chờ chủ dự án quyết: chờ ESAP 2027, xin quyền AMF, hay chỉ link thủ công).
- [ ] Phân tích tài chính (tỷ số, so sánh kỳ, nhận định) thuộc P6.

## P7 — nền tảng cảnh báo (offline, 11/11/2026, v0.2.0)

- [x] Migration 15: `alert_rules` (khớp region/topic/entity/keyword, mức, cooldown, giờ yên lặng + múi giờ), `alert_deliveries` **UNIQUE (rule, article)**, `alert_skip_log`.
- [x] Một sự kiện chỉ báo **đúng một lần**: quét lại nhiều lần không tạo thêm; có test.
- [x] Cooldown chặn chuỗi sự kiện rồi hết hạn; có test.
- [x] Giờ yên lặng (kể cả cửa sổ vắt qua nửa đêm) không làm **mất** sự kiện — bỏ qua không đánh dấu đã gửi; có test.
- [x] Luật chỉ khớp bài người đọc thấy: cùng điều kiện vùng bật + chủ đề bật; có test.
- [x] Chỉ kênh `in-app`. `telegram/email/webhook/imap/slack` bị từ chối `channel_not_implemented`, không lưu rồi im lặng.
- [x] API `GET/POST /api/admin/alerts/rules`, `PUT /api/admin/alerts/rules/{slug}`, `POST /api/admin/alerts/sweep`, `GET /api/alerts`, `POST /api/alerts/{id}/read`, `POST /api/alerts/read-all` (đều cần phiên admin/người đọc).
- [x] Quét nền 2 phút, khởi động ở lần đăng nhập admin đầu tiên; trước đó mọi luật tắt.
- [x] UI: trang **Cảnh báo** (hộp thư, chưa đọc, đánh dấu đã đọc, quét ngay) + khối **Luật cảnh báo** trong Cài đặt; VI/EN; Giấy/Sepia/Đêm; 375 px không tràn ngang.
- [x] 182 Rust test xanh (16 test alert + 2 test mức router); kiểm tay trên domain thật: quét hai lần → `created 1` / `created 0`.
- [ ] Gửi thật Telegram/email/webhook (kể cả ký HMAC) — **cần chủ dự án chọn kênh và cấp khóa**.
- [ ] Luật theo ngưỡng số liệu (giá/biến động/on-chain) — cần P2 có dữ liệu thật.
- [ ] Gộp theo story, chuông trên thanh điều hướng, cửa sổ im lặng theo lịch làm việc.

## P7 — kênh Telegram (12/11/2026, v0.4.1)

- [x] Khám phá API thật trước khi viết client: `telegram.lengoc.me` = `cygnus-gateway` (đọc 0.2.5.1, live 0.3.0.0 — đường gửi không đổi), `POST /send` `{method, payload}` hoặc body Bot API phẳng, auth `Authorization: Bearer <cyg_* api key>` / `x-token`; `payload` đi nguyên văn nên `message_thread_id` (forum topic) dùng được, không cần sửa relay. Không có `/api/send`; key có thể **ghim 1 bot** nên client không gửi trường `bot`. Có test hợp đồng khóa hình dạng request.
- [x] `telegram_mode` = `relay | direct | off` (`env_or_cfg`: `TELEGRAM_MODE` thắng setting); `TELEGRAM_RELAY_BASE_URL` chỉ https; `TELEGRAM_ALLOWED_HOSTS`; host relay fleet luôn được phép.
- [x] Secret ghi-một-chiều trong vault: `telegram-relay-token`, `telegram-bot-token`; `Endpoint` có `Debug` tự viết để log không in URL chứa token; API chỉ trả "đã có / chưa có".
- [x] Migration 16 `alert_telegram_outbox`; sweep chỉ ghi hàng đợi, flusher 30s gửi ngoài khoá DB, claim `pending → sending` trước khi gọi nên mỗi cảnh báo gửi tối đa một lần, hàng kẹt sau 300s được lấy lại, 5 lần thử rồi `failed`.
- [x] Nội dung = tiêu đề + link bài (đúng như hộp thư trong app); `alerts_topic_id` tuỳ chọn để vào topic riêng.
- [x] Luật được khai `telegram` ngay khi kênh bật, chưa cần secret; thiếu đích/khóa hiện thành `reason` + hàng đợi thay vì từ chối lưu luật.
- [x] Admin: `GET /api/admin/telegram`, `POST /api/admin/telegram/flush`, các trường trong `/api/settings`; UI khối "Kênh Telegram" (chế độ, chat id, topic, trạng thái sẵn sàng, hàng đợi, Gửi ngay).
- [x] Sửa lỗi sidebar bị giãn (chủ dự án báo): `.side-shelves { align-content: start }`; đo lại 1440×900: ô lọc 37px, "Mọi tin đang đọc" 37px, đầu nhóm 33px, mục 33px.
- [x] 203 test xanh; clippy `-D warnings` sạch; UI build sạch; deploy `df5f64d` → production 0.4.1.
- [ ] **Chờ chủ dự án chọn bot trên relay** (và chat/group id) — chưa có bot nào nên production để `telegram_mode=off`; chưa gửi thật vào chat nào.
- [ ] Nhận tin vào / định tuyến topic (chiều inbound) thuộc lane Library — app này chỉ gửi.

## UI + mobile (12/11/2026, v0.3.0)

- [x] Hàng nút dùng flex/grid + `gap` 10px, cho wrap; dưới 640px nhóm nút của tiêu đề xuống dưới thay vì bị ép dính nhau. Kiểm 360/375/390px, 6 trang: không cặp nút nào cách < 6px.
- [x] Vùng chạm ≥ 44px (nút, ô chọn, chip); công tắc giữ hình 46×26 nhưng nới vùng chạm bằng `::before`.
- [x] Thanh chuyển app: `ui/src/appLinks.ts` (bản sao byte-identical của lnc-library) → sidebar desktop, hàng dưới brand trên mobile; link Library `https://ebooks.lengoc.me`, đánh dấu app hiện tại.
- [x] Mobile: `viewport-fit=cover` + safe-area, thanh điều hướng đáy 5 mục (52px), `100dvh`, cuộn mượt, ô nhập 16px, thanh đáy ẩn khi bàn phím mở; bộ chọn vùng chuyển xuống thân trang để tiêu đề không bị bóp.
- [x] `manifest.webmanifest` + icon 192/512/maskable + `apple-touch-icon` + meta PWA (icon 32px đã soi, đọc được).
- [x] `.docs/mobile-apk-plan.md` — **chỉ kế hoạch**: Capacitor vs Tauri 2, màn hình nhập server + kiểm `/health`, API base URL, bearer vs cookie cross-site, cleartext Android, iOS cần macOS; 6 câu hỏi cho chủ dự án.
- [ ] Build APK thật — **chờ chủ dự án quyết** cách xác thực và phân phối.

## Đồng bộ giao diện với Library (12/11/2026, v0.4.0)

- [x] 6 nền dùng chung (Giấy, Sepia, Xám dịu, Xanh dịu, Đêm, Đen) đặt trên `:root[data-mode]`; `theme-color` đổi theo nền; danh sách mã trùng với `src/themes.rs` (server kiểm `default_reader_mode`).
- [x] Thanh tab dưới đáy dưới 900px thay sidebar, dùng đúng tên biến/lớp của Library (`--tabbar-h`, `--safe-*`, `--touch`, `.tabbar`, `.tab`).
- [x] Một lớp `.btn-row` cho mọi hàng nút; `.actions-row` giữ làm bí danh.
- [x] Sidebar desktop có khối duyệt: lọc nhanh + Chủ đề/Vùng/Nguồn kèm số đếm; chip bỏ lọc ở trang Chủ đề.
- [x] Trang chủ dạng dải ngang: Mới nhất, Cảnh báo gần đây, Theo dõi — dựng từ endpoint sẵn có.
- [x] Backend: `GET /api/articles?source=<slug>`; `/api/dashboard` trả số tin theo nguồn.
- [x] Kiểm trình duyệt thật 1280×800 và 375×812, cả 6 nền, không tràn ngang.
- [x] `.docs/mobile-apk-plan.md` viết lại cho khớp Library: Capacitor, màn hình địa chỉ server, bearer chỉ-đọc, không cookie cross-site, iOS = PWA; 6 quyết định dùng chung.

## Đo TRUSTED_PROXIES trên production (12/11/2026)

- [x] Đo trong container (`/proc/net/tcp`, chỉ ESTABLISHED): peer TCP là `172.31.0.1` (gateway Docker) cho **cả** đường domain lẫn `http://10.20.2.1:8462`. Vậy `TRUSTED_PROXIES=10.20.0.3` chưa từng khớp, limiter dùng chung một bucket trên production.
- [x] Ghi kết quả + 4 phương án (chưa áp dụng phương án nào) vào `PROJECT_STATUS.md`.
- [ ] Chủ dự án chọn phương án cho limiter (trust gateway / host network / đổi khoá đếm / nới hạn mức).

## P8 phần offline — `/mcp` (12/11/2026, v0.4.0)

- [x] `POST /mcp` JSON-RPC 2.0: `initialize`, `ping`, `tools/list`, `tools/call`, batch; cùng credential với REST v1; `GET /mcp` → 405 `stream_not_supported`.
- [x] 7 tool chỉ-đọc `newsman__*` (regions, topics, latest_articles, documents, tax_calendar, series, observations) — gọi chung hàm với `/api/v1/*`; không có tool ghi, không có tool cho dữ liệu chưa tồn tại.
- [x] Lỗi: method lạ `-32601`, tool lạ `-32602`, tham số/vùng sai → `result.isError = true` (không phải lỗi transport).
- [x] Tách `documents::list`, `series::list_in`, `regions::payload` để REST và MCP dùng chung một truy vấn.
- [x] `src/docs_tests.rs`: route ↔ `API_DOCS.md` khớp 100%, `mcp_available` khớp route `/mcp`; bổ sung các route alerts còn thiếu trong tài liệu.
- [x] 192 test xanh (7 MCP + 2 tài liệu), clippy `-D warnings` sạch.
- [x] Kiểm tay trên bản local: `initialize` → protocol `2025-06-18`, `serverInfo.name` `lnc-newsman`; không khóa → 401; `tools/list` 7 tên; gọi `latest_articles` → `isError:false`.
- [ ] Đăng ký `/mcp` lên Agent Hub — **chờ khóa Hub của chủ dự án**.

## P8 — nguồn văn bản pháp luật (12/11/2026, v0.4.2)

Yêu cầu qua lane Finance Overview, theo `.docs/legal-sources.md` (repo `lnc-library-ui`) §8: **Newsman phát hiện + cảnh báo, Library giữ toàn văn.** Ràng buộc cứng: kiểm điều khoản từng nguồn trước khi lấy, không tải hàng loạt, không dùng nguồn thương mại.

- [x] Đọc điều khoản 4 nguồn, ghi lại URL + ngày + trích nguyên văn: govinfo = `not_required` (GPO: public documents dùng lại được, cần ghi nguồn; có RSS theo bộ sưu tập, không khóa); Légifrance = `verified` (licence ouverte 2.0 + CGU PISTE + CGU API + quota, **cần đăng ký khóa**); EUR-Lex = `unverified` (trang thông báo trả 202 rỗng, feed "temporarily not available"); vbpl.vn = `unverified` (không có trang điều khoản đọc được, `robots.txt` chặn `/api/` và `/Pages/`).
- [x] Migration 17 `legal_sources`: `terms_verdict` + `terms_checked_on` + `terms_url` + `terms_quote` + `terms_note`, ràng buộc `CHECK`; mọi dòng khởi tạo `enabled=0`.
- [x] `src/legal.rs`: `verdict_allows_polling` chỉ nhận `verified`/`not_required`; `set_enabled` từ chối `terms_unverified`, `terms_prohibit_polling`, `no_documented_channel`, `region_disabled`; seed lại không bật lại và không ghi đè dòng chủ dự án đã sửa.
- [x] govinfo là nguồn duy nhất có kênh kiểm chứng được ⇒ bật nó sẽ tạo luôn source `legal-govinfo` (T1, 12 giờ/lần) cho collector; không viết thêm mã mạng nào.
- [x] API `GET /api/admin/legal-sources`, `PUT /api/admin/legal-sources/{code}`; thẻ "Nguồn văn bản pháp luật" trong Cài đặt có lý do chưa theo dõi + panel "Điều khoản" gấp gọn (trích dẫn + URL), cờ `evidence_stale` sau 180 ngày.
- [x] Chứng minh đầu-cuối rồi tắt lại: bật `us` + govinfo → fetch lưu **100 public law**, reader trả đúng tên nguồn/T1/link gốc; sau đó tắt govinfo, tắt `us`/`eu`, xoá 100 bài + source probe.
- [x] Hai lỗi UI thật gặp khi rà màn này: (1) Lightning CSS bỏ `backdrop-filter` khỏi bản build nên thanh tab dưới chỉ còn nền trong suốt 6% — chữ lộ qua; đã đổi sang nền đặc, đo lại 375×812; (2) nút "Bật theo dõi" hiện cả khi server chắc chắn từ chối — nay chỉ hiện khi bấm được.
- [x] **217 test xanh** (13 test legal + 1 test hình dạng feed govinfo thật), clippy `-D warnings` sạch, UI build sạch.
- [ ] Còn chờ chủ dự án: (a) đọc bằng chứng điều khoản trong thẻ Cài đặt rồi quyết có bật govinfo; (b) nếu muốn Légifrance → đăng ký PISTE lấy khóa + trả lời hạn mức; (c) EUR-Lex và vbpl.vn chỉ mở khi đọc được điều khoản của trang (hoặc có kênh công bố chính thức).

### Bật govinfo trên production — chặn bởi egress của NAS (12/11/2026)

Chủ dự án duyệt bật govinfo. Đã làm đúng thứ tự và **hoàn lại trạng thái tắt** sau khi phát hiện chặn:

- [x] Bật vùng `us` → 200; bật `govinfo` → 200. Registry đúng, source collector **`legal-govinfo` (id 6, T1, 12 giờ/lần, chủ đề politics)** được tạo tự động như thiết kế.
- [x] Lượt fetch đầu: `{"status":"error","error":"timeout","items_new":0}` sau **10 575 ms** — đúng bằng `connect_timeout` 10 s của `fetch::Fetcher`. `consecutive_failures = 1`, tự tạm dừng ở 5.
- [ ] **Nguyên nhân không nằm ở Newsman**: container `lnc-newsman` (172.31.0.2) không mở được TCP ra ngoài. Đo từ chính container: `1.1.1.1:443` fail, `8.8.8.8:53` fail, `104.17.120.181:443` (govinfo) fail, gateway `172.31.0.1` từ chối. DNS vẫn phân giải được (cả IPv4 và IPv6), nên đây là chặn ở tầng chuyển tiếp/NAT chứ không phải DNS.
- **Đối chứng** (cùng lúc, cùng host, cùng lệnh): `immich_server` 172.19.0.5 → **200**; `kavita` 172.21.0.2 → **200**; `vaultwarden` 172.22.0.2 → **200**; `lazylibrarian` 172.18.0.4 → **200**; `fail2ban` 172.28.0.2 → **200**; `couchdb` 172.17.0.2 → **200**. Ngược lại `lnc-library` 172.29.0.2 → **FAIL**, `lnc-cwa-trial` 172.30.0.2 → **FAIL**. Container network `host` (`wolweb`) ra ngoài **OK**.
- Cả ba mạng hỏng (`172.29`, `172.30`, `172.31`) đều được tạo trong ~40 giờ gần đây (12/11 12:36, 12/11 00:21, 12/11 16:12); các mạng còn chạy được đều cũ hơn. Từ trong `lnc-newsman` gọi **cùng host** `172.18.0.3:8081` thì **OPEN**, nhưng gọi LAN `10.20.1.105:9443`, firewall `10.20.0.1` hay `1.1.1.1:443` đều **FAIL** — tức nó không ra khỏi bridge được, dù `container→container` cùng host vẫn thông. Bridge đều `up`, `carrier=1`, `mtu=1500`; `ip_forward=1` và `forwarding=1` cho mọi bridge ⇒ loại trừ MTU và loại trừ thiếu forwarding toàn cục.
- **Ma trận sạch** (mã thoát: 0 = mở, 1 = refused, 124 = timeout), ba container ở dải hỏng và ba container ở dải chạy được, cùng 6 đích:

  | Đích | 172.29 / 172.30 / 172.31 (hỏng) | 172.17 / 172.18 / 172.19 (chạy) |
  |---|---|---|
  | Portainer trên chính host `10.20.2.1:19943` | **0** (mở) | 0 |
  | Cổng đóng trên chính host `10.20.2.1:22` | 1 (refused) | 1 |
  | Container khác bridge cùng host `172.18.0.3:8081` | **0** (mở) | 0 |
  | OPNSense `10.20.0.1:53` | **124** (timeout) | 0 |
  | VM khác `10.20.1.105:9443` | **124** | 0 |
  | `1.1.1.1:443` | **124** | 0 |

  Đây là dấu hiệu rất đặc trưng, và nó **hẹp hơn** kết luận trước: tới chính host và tới container ở bridge khác đều **thông** (nên định tuyến/forwarding giữa các bridge **vẫn làm việc**), nhưng mọi đích **ra khỏi host** đều timeout. Đúng kiểu nguồn `172.29/30/31` không được masquerade nên gói trả về không tìm được đường về.
- **Vẫn chưa đọc được luật NAT thật**: `iptables -t nat -S` trong netns `netbird-nas` chỉ ra chain rỗng và cảnh báo "iptables-legacy tables present"; ADM của ASUSTOR lại dùng bảng khác thường. Kết luận "thiếu MASQUERADE" giờ **phù hợp mạnh với bằng chứng** nhưng vẫn là suy ra từ hành vi, chưa phải đọc trực tiếp luật — session chẩn đoán read-only trên NAS sẽ xác nhận.
- [x] Đã **tắt lại** `govinfo` và vùng `us` (200 cả hai). Bốn nguồn luật trở về `enabled=false`, đúng như bản phát hành 0.4.2. Không có bài nào được lưu, không có cấu hình nào bị đổi ngoài hai công tắc vừa hoàn lại.
- [ ] **Cần chủ dự án / hạ tầng**: khôi phục NAT cho các bridge mới trên NAS Saigon (ASUSTOR ADM, Docker 29.6.2) — hoặc cho Newsman chạy `network_mode: host` như `lnc-social-dl`/`wolweb`. Không tự sửa firewall/NAT của NAS: ngoài phạm vi và không có phê duyệt cho thay đổi hạ tầng dùng chung. Sau khi thông, bật lại chỉ là hai nút và lượt fetch sẽ lưu ~100 public law như đã chứng minh ở local.

## Rà soát giao diện cuối (checklist dùng chung, mục U)

Nhận từ lane Library (`.docs/task.md` bên đó, mục "U — Rà soát giao diện cuối"). Nguyên tắc: app cho người đọc cuối — không lỗi cân đối, không chữ kiểu ghi chú kỹ thuật.

- [ ] **Sidebar/kệ chủ đề-nguồn**: cân đối như Library — đã sửa `align-content: start` (v0.4.1), đo lại 1440×900: ô lọc 37 px, mục 33 px; còn phải rà nhãn nhóm và số đếm khi có dữ liệu thật.
- [ ] **Trang đọc tin + cài đặt nguồn/vùng**: không còn chữ kỹ thuật, empty-state rõ ở trang chủ (đã có câu dẫn "Hãy thêm nguồn RSS/Atom…"); rà lại phần Cài đặt một lượt khi có dữ liệu thật.
- [ ] **Trang alerts/briefs**: badge mức độ nhất quán, không lộ id/field thô.
- [ ] **Mobile 375×812**: không scroll ngang; dropdown không bị cắt trong card. Đã kiểm thẻ Nguồn văn bản pháp luật (không tràn ngang); còn rà các màn còn lại.
- [ ] **Song ngữ vi/en**: duyệt lại toàn bộ key — thẻ mới đã có đủ cả hai ngôn ngữ.

## P6–P9

Xem bảng giai đoạn trong `plan.md` mục 5: P2 kho secret + connector (Bitcoin/on-chain, vĩ mô), P3 văn bản chính thức & thuế, P4 chứng khoán, P5 LLM, P6 phân tích & nhận định, P7 cảnh báo, P8 REST/MCP/Hub, P9 triển khai NAS. Các quyết định cần anh chốt nằm ở `plan.md` mục 8.

## Hành động trên hệ thống đang chạy

Chưa đăng ký agent lên Agent Hub, tạo repo/registry, cấp DNS hay deploy Portainer. Những thay đổi này cần được anh duyệt trong ngữ cảnh tương ứng.
