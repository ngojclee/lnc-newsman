# LNC Newsman — Kế hoạch triển khai

- Repo: `D:\Python\projects\lnc-newsman` · stack Portainer `lnc-newsman` · domain `newsman.lengoc.me`
- Host đích: `10.20.2.1` (Saigon NAS, ASUSTOR) · `DATA_DIR=/share/Docker` · cổng host dự kiến `8462`
- Ngày lập: 11/11/2026 · **Phiên bản kế hoạch: 0.2** (11/11/2026 — mở rộng sang hub dữ liệu tài chính, on-chain, chứng khoán, văn bản chính thức/thuế theo vùng)
- Nguồn gốc: kế thừa phương pháp của `D:\_Finance` (nguồn đã chấm tier, timeline sự kiện, scorecard dự báo). Nhập dữ liệu từ dự án đó cần duyệt riêng.

## 1. Mục tiêu

Newsman là **hub thu thập và phân tích thông tin** cho người dùng nội bộ, ưu tiên theo thứ tự:

1. **Tài chính & thị trường**: tin tài chính, chứng khoán (chỉ số, mã theo dõi, công bố thông tin doanh nghiệp), vĩ mô.
2. **Bitcoin & on-chain**: giá, dữ liệu mạng lưới (phí, mempool, hashrate, độ khó, khối), dòng tiền ETF, tin crypto.
3. **Tin thế giới** và chính sách có ảnh hưởng thị trường.
4. **Văn bản chính thức & thuế** ở các vùng anh có kinh doanh: luật, nghị định, thông tư, hướng dẫn thuế, lịch thuế.

Hệ thống tự quét theo lịch, xếp theo vùng và chủ đề, tóm tắt, rồi đưa ra **bản phân tích/nhận định có trích nguồn**, báo cáo định kỳ và cảnh báo. Bên ngoài lấy dữ liệu qua REST API và MCP.

Sản phẩm chính là **trang đọc và trang phân tích**. Quản trị, API và MCP là tính năng phụ.

## 2. Quyết định đã chốt với anh

| Hạng mục | Quyết định |
|---|---|
| Phạm vi tin | Đa lĩnh vực; ưu tiên tài chính, Bitcoin/on-chain, chứng khoán, thế giới, văn bản chính thức/thuế (11/11) |
| Vùng hoạt động | **Pháp, Mỹ, Việt Nam**; Pháp là con của **EU** nên tự lấy cả thông tin EU (11/11) |
| Mở rộng vùng | Thêm một **thư mục vùng** là job tự nhận vùng mới, không sửa code (11/11) |
| Kênh cảnh báo | Chuông trong app + Telegram + Email digest + Webhook ra ngoài |
| Ngôn ngữ | Trang đọc song ngữ VI/EN, mặc định VI |
| Người dùng | Nhiều người đọc có tài khoản riêng + 1 admin |
| Công nghệ | Rust `axum` + React/Vite (đã duyệt khi dựng P0) |

## 3. Quyết định kỹ thuật (em chọn theo blueprint, anh đổi được)

| Hạng mục | Chọn | Lý do |
|---|---|---|
| Backend | Rust `axum` + `tokio` + `rusqlite` (SQLite, FTS5) | Rust-first; NAS ít RAM |
| UI | React + Vite, phục vụ tĩnh từ Rust | Không chạy Node lúc production |
| Tin tức | RSS/Atom trước (đã có), HTML lịch sự + `lnc-scrapling` sau | Rẻ, ít bị chặn |
| Dữ liệu số (giá, vĩ mô, on-chain) | **Connector** riêng cho từng nhà cung cấp, lưu chuỗi thời gian | Tách khỏi kho bài; mỗi nguồn có hạn mức riêng |
| Cấu hình vùng | **Region pack** dạng thư mục lồng nhau trong `/data/regions` | Thêm vùng = thêm thư mục |
| LLM | OpenAI-compatible qua LNC Agent Hub | Một chỗ quản model/key/hạn mức |
| Agent | `news-analyst-argus` (đã có trong AgentSync) | Nghiên cứu sâu khi luật hoặc người dùng yêu cầu |
| Deploy | Image-pull, build trên runner `docker-build` | Build Rust trên NAS chậm |

## 4. Kiến trúc

```
Region packs (/data/regions/**)  ─┐
Danh mục trong UI                 ├─► Bộ lập lịch theo vùng đang bật (gồm vùng cha)
                                  │
   ┌──────────────┬───────────────┼──────────────────┬────────────────────┐
   ▼              ▼               ▼                  ▼                    ▼
 Tin RSS/HTML   Văn bản chính   Thị trường         Vĩ mô               On-chain BTC
 (bài báo)      thức & thuế     (chỉ số, mã, công  (lãi suất, CPI…)    (phí, mempool,
                                bố thông tin)                          hashrate, ETF)
   │              │               │                  │                    │
   ▼              ▼               ▼                  ▼                    ▼
 Kho bài      Kho văn bản      Chuỗi thời gian (series + observations) + sự kiện
   └──────────────┴──────────┬────┴──────────────────┴────────────────────┘
                             ▼
          Phân loại vùng/chủ đề/thực thể → gom story → tóm tắt (luật trước, LLM sau)
                             ▼
          Phân tích & nhận định có trích nguồn · báo cáo · cảnh báo · scorecard dự báo
                             ▼
          Trang đọc/Phân tích · REST /api/v1/* · /mcp
```

### 4.1 Nguồn và độ tin cậy (đã có phần lõi)

- Mỗi nguồn: tier T1–T4, ngôn ngữ, **vùng**, chủ đề mặc định, loại nguồn, lịch quét, trạng thái.
- Theo dõi sức khỏe nguồn; lỗi 5 lần liên tiếp tự tạm dừng (đã có).
- Tier ảnh hưởng điểm quan trọng và luật "cần ≥ N nguồn độc lập xác nhận".

### 4.2 Bậc thang fetch

1. RSS/Atom có `ETag`/`If-Modified-Since` — **đã có**, kèm chặn địa chỉ nội bộ.
2. HTTP trực tiếp: giới hạn theo host, tôn trọng `robots.txt`, backoff khi 429/5xx.
3. `lnc-scrapling` cho trang dựng bằng JavaScript.
4. API chính thức có khóa (connector dữ liệu, mục 4.11).
5. Agent `news-analyst-argus` cho nhiệm vụ nghiên cứu.

Không vượt paywall, không giải CAPTCHA, không đăng nhập hộ, tuân thủ điều khoản của từng nhà cung cấp dữ liệu.

### 4.3 Bản quyền nội dung

Chỉ lưu tiêu đề, URL, metadata và tóm tắt tự viết; không sao chép hoặc lưu toàn văn bài báo. Văn bản pháp luật công khai được lưu số hiệu, tiêu đề, ngày hiệu lực, link gốc và tóm tắt tự viết. Số liệu thị trường chỉ lưu và hiển thị trong phạm vi giấy phép của nhà cung cấp.

### 4.4 Topic, vùng và phân loại

- Hai trục độc lập: **chủ đề** (cây: Tài chính › Chứng khoán, Crypto › Bitcoin, Thuế, Thế giới…) và **vùng** (cây vùng, mục 4.10).
- Một mục mang nhiều nhãn. Thứ tự: luật nguồn/từ khóa/thực thể → LLM cho ca mơ hồ → người đọc sửa nhãn.
- Thực thể: tổ chức, người, mã chứng khoán (`EPA:MC`, `NASDAQ:AAPL`, `HOSE:VCB`), tài sản (`BTC`), cơ quan ban hành.
- Trạng thái kiểm chứng `verified` / `reported` / `unverified` theo quy ước của `D:\_Finance`.

### 4.5 LLM

- Mỗi tác vụ chọn model riêng: phân loại (rẻ), tóm tắt, phân tích/báo cáo (mạnh).
- Hạn mức token/ngày, cache theo hash nội dung; chế độ "không LLM" vẫn chạy bằng luật.
- Prompt có phiên bản; `base_url` qua allowlist để tránh SSRF.

### 4.6 Cảnh báo

- Luật theo: vùng, chủ đề, từ khóa, thực thể, tier, độ quan trọng, số nguồn xác nhận.
- **Luật số liệu**: ngưỡng giá/biến động (BTC, chỉ số, mã theo dõi), on-chain (phí tăng đột biến, hashrate giảm mạnh, mempool tắc), vĩ mô (công bố lãi suất, CPI lệch dự báo).
- **Luật văn bản**: văn bản thuế mới ở vùng đang bật, hạn nộp thuế sắp tới, công bố thông tin của mã trong watchlist.
- Mức: thông tin / quan trọng / khẩn; cooldown, gộp theo story, giờ yên lặng.
- Kênh: chuông, Telegram, email digest, webhook ký HMAC.

### 4.7 Trang đọc và phân tích

- Ba chế độ đọc Giấy/Sepia/Đêm (đã có).
- Trang: Hôm nay, **Thị trường** (bảng chỉ số, watchlist, biểu đồ), **Bitcoin & on-chain**, **Vùng** (tin + văn bản + lịch thuế theo từng vùng), Chủ đề, Story, Báo cáo, Cảnh báo, Timeline, Đã lưu, Tìm kiếm.
- Bộ chọn vùng ở thanh trên (đã có ở P1.5): chọn Pháp hiển thị cả mục của EU và Toàn cầu.
- Dùng tốt trên điện thoại; đánh dấu đã đọc theo từng người.

### 4.8 API và MCP

- REST `/api/v1/*`: `articles` (đã có, thêm tham số `region` ở P1.5), `regions` (đã có, chỉ vùng đang bật), `stories`, `topics`, `regions`, `sources`, `series`, `observations`, `documents`, `reports`, `analyses`, `alerts`, `timeline`, `search`.
- `/mcp` theo chuẩn Business MCP Hub, namespace `newsman__*`. Tool dự kiến: `news_latest`, `news_search`, `story_get`, `region_brief`, `market_snapshot`, `series_get`, `onchain_snapshot`, `tax_updates`, `documents_search`, `report_latest`, `analysis_get`, `alerts_list`, `timeline_events`, `research_request`.

### 4.9 Bảo mật và cấu hình

- Admin: `MANAGEMENT_KEY` → phiên HMAC (đã có). Người đọc: tài khoản + argon2. API key chỉ đọc (đã có).
- **Kho secret mã hóa (đã có ở P2 nền tảng):** giá trị được niêm phong bằng XChaCha20-Poly1305 dưới `LNC_MASTER_KEY` (64 ký tự hex hoặc 32 byte base64); nonce 24 byte ngẫu nhiên **mỗi lần ghi**, tên khóa được ràng buộc làm associated data nên không thể chuyển ciphertext sang tên khác. `LNC_MASTER_KEY_OLD` giữ khóa cũ để đọc trong lúc xoay vòng; `POST /api/admin/secrets/rewrap` giải mã rồi niêm phong lại bằng nonce mới, và **hủy toàn bộ** nếu có một dòng không mở được bằng khóa hiện tại/cũ (không bao giờ ghi đè mất khóa). Khóa không thiết lập thì kho đóng (503), không bao giờ lưu plaintext. API/UI chỉ trả tên, phiên bản khóa, thời điểm cập nhật và fingerprint ngắn — không có thao tác "xem lại".
- Bộ tải ra ngoài dùng chung một lớp chặn SSRF (đã có), áp dụng cả cho connector.

### 4.10 Vùng và region pack (mới)

**`global` là gốc ảo/ngầm**, không phải một thư mục anh em với `eu`/`us`/`vn`. Thư mục gốc của region pack **chính là** vùng `global`; nó luôn tồn tại, luôn bật và không tắt được. Mọi thư mục ở cấp 1 đều là con của `global`.

Cây vùng mặc định:

```
global                 ← gốc ảo: chính thư mục gốc của pack, luôn bật
├─ eu                  (Liên minh châu Âu)
│  └─ fr               (Pháp)
├─ us                  (Hoa Kỳ)
└─ vn                  (Việt Nam)
```

- Anh bật vùng **lá** (fr, us, vn). Tập vùng hiệu lực = vùng đã bật + **tất cả vùng cha** (fr → eu → global). Job quét, báo cáo, `/api/articles`, `/api/v1/articles` và cảnh báo đều dùng tập này.
- Bật vùng con yêu cầu vùng cha đang bật; tắt vùng cha yêu cầu mọi vùng con đã tắt (giống cây chủ đề). `global` không tắt được.
- **Thư mục lồng nhau chính là quan hệ cha/con.** Thêm `asia/sg/` thì `sg` là con của `asia`; `asia` là con của `global`. Một thư mục chỉ được nhận là vùng khi có `region.toml`; thư mục cha thiếu `region.toml` làm vùng con bị báo lỗi và bỏ qua. Mã vùng lấy từ tên thư mục (chữ thường, số, gạch ngang).
- Cấu trúc pack:

```
/data/regions/
  region.toml                 sources.toml        ← vùng global (gốc)
  eu/region.toml              sources.toml        (khi làm P2–P4: official.toml, macro.toml)
  eu/fr/region.toml           sources.toml        (khi làm P3–P4: official.toml, tax.toml, markets.toml)
  us/region.toml              ...
  vn/region.toml              ...
```

  - `region.toml`: `code` (tùy chọn, phải khớp tên thư mục; ở gốc là `global`), `name_vi`, `name_en`, `currency`, `timezone`, `language`.
  - `sources.toml`: danh sách `[[source]]` với `slug`, `name`, `url`, `tier`, `language`, `topic`, `interval_minutes`. P1.5 chỉ nạp hai file này; official/tax/markets/macro để P2–P4.
- **Phát hiện ≠ kích hoạt.** Quét thư mục khi khởi động và khi bấm **Nạp lại vùng** chỉ *đăng ký* vùng và nguồn ở trạng thái **tắt** (trừ `global`). Vùng/nguồn chỉ chạy job và truy cập mạng sau khi admin bật trong UI. Nguồn mới thêm từ pack luôn ở trạng thái tắt, kể cả khi đã có vùng bật.
- Kiểm tra từng file: lỗi cú pháp TOML, mã trùng, mã sai, cha không tồn tại, chu trình, URL không hợp lệ, trùng nguồn thủ công → **bỏ qua file/vùng đó**, ghi lỗi trong UI, không làm hỏng vùng khác và **giữ cấu hình hợp lệ của lần nạp trước** (không xóa vùng/nguồn đã có). Bỏ qua symlink và mọi đường dẫn thoát khỏi thư mục gốc.
- Mục từ pack được đánh dấu "do pack quản lý": trong UI chỉ bật/tắt, sửa nội dung thì sửa file. Nguồn thêm tay trong UI vẫn được gán vùng như thường.
- Image mang sẵn bộ pack mặc định (`global`, `eu`, `eu/fr`, `us`, `vn`) nhúng trong binary; bản trong `/data/regions` được ưu tiên để sửa không cần build lại. Đường dẫn pack cấu hình bằng `REGIONS_DIR` (mặc định `<DATA_DIR>/regions`).

### 4.11 Dữ liệu thị trường, vĩ mô và on-chain (nền tảng P2 đã có, connector thật chưa làm)

**Đã triển khai (P2 nền tảng, test giả lập, chưa nghiệm thu với dữ liệu thật):**

- Kho chuỗi thời gian: `series` (mã, nhà cung cấp, vùng, đơn vị, tần suất) + `observations`
  (kỳ dữ liệu `observed_at`, **thời điểm biết** `as_of`, `revision`, giá trị, chất lượng
  preliminary/final/revised, nguồn) + `series_events` (công bố, nghỉ lễ, hồ sơ).
  - Sửa số liệu = **thêm revision**, không ghi đè; truy vấn `as_of` trả đúng những gì
    biết tại thời điểm đó. Ghi lặp cùng giá trị là no-op.
  - Mỗi chuỗi thuộc một vùng; API đọc chỉ trả chuỗi thuộc tập vùng hiệu lực.
- Khung **connector** (`src/connectors.rs`): trait `Connector` khai báo secret cần dùng,
  khoảng chạy tối thiểu, hạn mức/ngày, cache; connector **bắt buộc** nhận `fetch::Fetcher`
  của hệ thống nên mọi request đi qua cùng lớp chặn SSRF. Các cổng chặn được áp bởi khung,
  không phụ thuộc connector: connector phải bật, vùng phải bật, secret phải có và giải mã
  được, chưa tới lịch, chưa hết hạn mức ngày, cache còn mới. Mọi lần chạy (kể cả khi bị
  chặn) đều tính vào hạn mức và ghi sức khỏe/lỗi vào bảng `connectors`.
- API đọc `/api/v1/series`, `/api/v1/series/{code}/observations?as_of=`, `/api/v1/events`;
  UI khung **Thị trường** (danh sách chuỗi + điểm mới nhất) và hai khối quản trị
  **Khóa nhà cung cấp** (chỉ ghi) và **Connector** (sức khỏe, hạn mức).
- Kho secret: xem mục 4.9.

**Connector thật đã viết (11/11/2026, theo bộ nguồn anh chọn ở `.docs/sources-candidates.md` mục 6):**

| Connector | Vùng | Khóa | Hạn mức | Đã test giả lập | Đã nghiệm thu |
|---|---|---|---|---|---|
| `mempool-space` | global | không | 10 phút/lần, 144/ngày, 4 request/lần | có (loopback) | **chưa** |
| `ecb-sdmx` | eu | không | 6 giờ/lần, 12/ngày, 1 request/lần | có | **chưa** |
| `federal-register` | us | không | 6 giờ/lần, 6/ngày, ≤2 trang/lần | có | **chưa** |
| `fred` | us | `fred-api-key` | 12 giờ/lần, 6/ngày, 1 request/lần | có | **chưa** |
| `coingecko` | global | `coingecko-api-key` | ≤2/phút, 30/ngày, dừng ở 450/tháng | có | **chưa** |
| `sec-edgar` | us | `SEC_USER_AGENT` (cấu hình, không phải khóa) | 1/phút, 60/ngày, 1 request/lần | có | **chưa** |

Mọi connector mặc định **tắt** và chỉ chạy khi vùng + connector đều bật; mọi request đi qua
`fetch::Fetcher`. Hạn mức đếm theo **từng yêu cầu HTTP**, không theo lần chạy. Mỗi giá trị hiển
thị kèm dòng ghi nguồn theo điều khoản nhà cung cấp; ECB dùng đúng nguyên văn
"Source: ECB statistics".

**Chưa làm:** lịch chạy tự động cho connector (hiện admin bấm `POST /api/admin/connectors/{id}/run`),
biểu đồ, lưu filing SEC (mới đếm entry), chứng khoán VN/Pháp, ETF Farside, Légifrance. Kịch bản
nghiệm thu trực tiếp: `.docs/live-acceptance.md` — **chưa chạy, chờ anh duyệt riêng**.

- Ứng viên nhà cung cấp — **chưa kiểm chứng điều khoản và endpoint; cần anh chọn**:

| Nhóm | Ứng viên | Ghi chú |
|---|---|---|
| Bitcoin on-chain | `mempool.space` API; node Bitcoin Core tự chạy (tùy chọn) | Phí, mempool, khối, hashrate, độ khó |
| Giá crypto | CoinGecko (gói miễn phí có hạn mức) | Giá BTC theo USD/EUR/VND |
| Dòng tiền ETF BTC | Trang công bố của nhà phát hành | Cần xem điều khoản |
| Chứng khoán Mỹ | SEC EDGAR (công bố chính thức, miễn phí); nhà cung cấp giá có khóa | EDGAR yêu cầu User-Agent nhận diện |
| Chứng khoán Pháp/EU | Euronext, AMF (công bố); nhà cung cấp giá có khóa | Dữ liệu giá thời gian thực thường có phí |
| Chứng khoán Việt Nam | HOSE/HNX, UBCKNN (công bố); nhà cung cấp giá | API công khai ổn định còn hạn chế |
| Vĩ mô | FRED (Mỹ), ECB Data Portal & Eurostat (EU), INSEE (Pháp), Tổng cục Thống kê & NHNN (VN) | Nhiều nguồn miễn phí, một số cần khóa |

- Không dùng nguồn cào trái điều khoản (ví dụ API không chính thức của cổng tài chính lớn).

### 4.12 Văn bản chính thức và thuế (mới)

- Kho `documents`: vùng, cơ quan ban hành, loại (luật, nghị định, thông tư, hướng dẫn thuế, thông cáo), số hiệu, ngày ban hành, ngày hiệu lực, link gốc, tóm tắt tự viết, chủ đề (thuế doanh nghiệp, VAT, thuế thu nhập, hải quan, thương mại điện tử…).
- Ứng viên nguồn — **chưa kiểm chứng endpoint**: EU: EUR-Lex, Commission (TAXUD); Pháp: Légifrance/Journal officiel, BOFiP, impots.gouv; Mỹ: Federal Register, IRS newsroom, Treasury; Việt Nam: Công báo, Cổng TTĐT Chính phủ, Bộ Tài chính, Cục Thuế.
- **Lịch thuế** theo vùng (hạn nộp VAT, thuế doanh nghiệp, kê khai…) khai báo trong `tax.toml`, nhắc trước N ngày.
- **Trạng thái P3 nền tảng (11/11/2026, offline):**
  - Pack có `official.toml` (cơ quan ban hành) và `tax.toml` (nguồn thuế + hạn nộp định kỳ) cho
    global/eu/eu-fr/us/vn, kiểm tra từng file như `sources.toml` (cú pháp, mã trùng, loại sai,
    cadence sai, nguồn không tồn tại, ngày ngoài khoảng). Mọi mục mặc định **TẮT**, không ra mạng.
  - Kho `documents` có `topic`, `disclaimer`; tìm kiếm FTS5 bỏ dấu tiếng Việt (`thue` khớp `thuế`),
    mọi tóm tắt do người viết đều tự động mang dòng "không phải tư vấn pháp lý/thuế" do server đặt.
  - Engine lịch thuế tính hạn theo cadence (tháng/quý/năm), múi giờ của vùng, quy tắc cuối tuần do
    pack khai báo, kẹp ngày vào cuối tháng ngắn; API `/api/v1/tax-calendar` + bật/tắt từng rule.
    **Ngày lễ công cộng chưa mô hình hoá** — nêu rõ trong API và UI.
  - UI trang **Vùng**: tin + văn bản (tìm kiếm) + lịch thuế theo tập vùng hiệu lực, 3 chế độ đọc.
  - **Chưa nghiệm thu**: các hạn nộp trong pack là **dữ liệu mẫu** (`verified = false`) và chưa
    đối chiếu luật từng vùng; chưa gửi Telegram/email (P7).
- Mọi tóm tắt văn bản ghi rõ "không phải tư vấn pháp lý/thuế"; luôn dẫn văn bản gốc.

### 4.13 Phân tích và nhận định (mới)

- **Bản tin phân tích** theo lịch: sáng mỗi ngày (theo vùng), tuần (thị trường, Bitcoin, thuế), và theo yêu cầu.
- Cấu trúc mỗi nhận định: bối cảnh → dữ liệu (có số liệu và nguồn) → các kịch bản (tăng/cơ sở/giảm) kèm điều kiện và mức tin cậy → điều cần theo dõi. Phân biệt rõ **sự kiện đã xác minh** với **suy luận**.
- Nhận định có thể kiểm chứng sau được đưa vào **scorecard dự báo**, chấm lại khi đến hạn để đo độ chính xác của mô hình/prompt.
- Ranh giới: đây là **công cụ nghiên cứu**, không phải khuyến nghị đầu tư cá nhân; không có chức năng đặt lệnh hay kết nối tài khoản giao dịch. Mỗi bản phân tích có ghi chú này.

## 5. Các giai đoạn — tiêu chí nghiệm thu, thứ tự, phụ thuộc, rollback

Quy ước trạng thái dùng chung: **đã chốt** = chủ dự án đã quyết; **đề xuất** = em nêu, chưa
được duyệt; **đã làm** = có mã + test giả lập; **đã nghiệm thu** = đã chạy với nguồn thật và có
log. Chỉ cột "đã nghiệm thu" mới được coi là xong.

### 5.1 Bảng tổng hợp

| GĐ | Việc | Phụ thuộc (phải xong trước) | Nghiệm thu đo được | Trạng thái |
|---|---|---|---|---|
| P0 | Khung Rust + UI + auth + 3 chế độ đọc + Docker | — | `cargo test` xanh; `/health` trả 200; container chạy chỉ bằng biến môi trường | **đã nghiệm thu**: stack 108 healthy trên NAS, `/health` 200 qua domain |
| P1 | Sổ nguồn, fetch RSS an toàn, scheduler, kho bài | P0 | ≥ 20 nguồn thật, 0 bài trùng theo canonical URL, 1 nguồn lỗi 5 lần tự dừng; log chứng minh | đã làm; **chưa nghiệm thu** |
| P1.5 | Cây vùng + region pack | P1 (danh mục nguồn) | Thêm thư mục vùng mới → xuất hiện ở trạng thái tắt sau khi nạp; chọn FR trả cả EU; 39 test | đã làm + chủ dự án duyệt; **nguồn thật chưa nghiệm thu** |
| P2 | Kho secret, khung connector, chuỗi thời gian, connector đầu | P1.5 (vùng) | Xem 5.3 | đã làm + duyệt; **nhà cung cấp thật chưa nghiệm thu** |
| P3 | Văn bản chính thức & thuế, lịch thuế | P1.5 | Xem 5.4 | nền tảng đã làm + duyệt; **hạn nộp chưa xác minh** |
| P4 | Chứng khoán: chỉ số, watchlist, công bố | P2 (series) + P3 (documents) | Xem 5.5 | **đề xuất**, chưa làm |
| P5 | Phân loại + tóm tắt bằng LLM, gom story | P1 (bài) + P2 (secret cho khóa LLM) | Xem 5.6 | nền tảng offline **đã làm** (nhãn luật + API + UI, đo 96%/84%); **chưa có LLM thật, chưa tóm tắt/gom story** |
| P6 | Phân tích/nhận định + báo cáo + scorecard | P5 (phân loại) + P2 (số liệu) | Xem 5.7 | **đề xuất** |
| P7 | Cảnh báo: chuông, Telegram, email, webhook | P5 + P6 (nội dung để cảnh báo) | Xem 5.8 | nền tảng offline **đã làm** (luật + hộp thư trong app); **Telegram/email/webhook chưa làm** |
| P8 | REST v1 đầy đủ + `/mcp` + đăng ký Hub | P4–P7 (các endpoint nguồn) | Xem 5.9 | **đề xuất** |
| P9 | Phát hành lên `10.20.2.1` | P0–P8 theo phạm vi phát hành | Xem 5.10 | **một phần đã làm**: stack repo-build + HTTPS nội bộ đang chạy; sao lưu/khôi phục chưa thử |

Thứ tự bắt buộc: `P0 → P1 → P1.5 → (P2 ━ P3) → P4` và `P1 → P5 → (P6 → P7) → P8 → P9`.
P2 và P3 độc lập với nhau (chỉ cùng phụ thuộc P1.5), nên đổi thứ tự được. P5 chỉ cần P1 và một
khóa LLM trong kho secret; nó **không** cần P2 xong hoàn toàn.

### 5.2 Cách một giai đoạn được coi là "đã nghiệm thu"

Một giai đoạn đạt nghiệm thu khi có **cả ba**:
1. lệnh kiểm chứng ghi trong mục tương ứng chạy xanh trên máy chủ đích hoặc local sạch;
2. một lần chạy với **dữ liệu thật** (hoặc nhà cung cấp thật) có log: thời điểm, số yêu cầu,
   số bản ghi, lỗi (nếu có);
3. kết quả ghi vào `.docs/PROJECT_STATUS.md` kèm ngày.

Thiếu (2) thì trạng thái là "đã làm + test giả lập", không phải "đã nghiệm thu".

### 5.3 P2 — dữ liệu số (đã làm nền tảng, chờ nghiệm thu thật)

- **Đã có (đã làm + duyệt)**: kho secret XChaCha20-Poly1305 + rewrap; bảng `series`/`observations`
  (as-of, revision)/`series_events`; khung connector áp cổng, hạn mức theo request, cache, health;
  6 connector: `mempool-space`, `ecb-sdmx`, `federal-register`, `fred`, `coingecko`, `sec-edgar`.
- **Nghiệm thu đo được** (khi chủ dự án duyệt `.docs/live-acceptance.md`):
  - mỗi connector chạy **một lần** đúng số yêu cầu ghi trong kịch bản (4/1/2/1/1/1);
  - 0 lỗi HTTP khác 200; 0 lần chạm 429;
  - mọi giá trị có `attribution`; khóa/email không xuất hiện trong DB, log và API (kiểm bằng grep);
  - `requests_today` sau lần chạy bằng đúng số yêu cầu thực tế.
- **Rủi ro + rollback**: xem mục 6. Rollback riêng: tắt connector (`enabled=0`), xoá khóa trong
  kho secret; dữ liệu đã ghi vẫn đọc được vì dữ liệu là bảng riêng.

### 5.4 P3 — văn bản chính thức và thuế (nền tảng đã làm, dữ liệu chưa xác minh)

- **Đã có (đã làm + duyệt)**: pack `official.toml`/`tax.toml`; `documents` có `topic` và
  `disclaimer`; FTS5 bỏ dấu; engine lịch thuế; API `/api/v1/tax-calendar`; UI trang Vùng.
- **Nghiệm thu đo được**:
  - **bắt buộc trước**: mỗi rule trong `tax.toml` được chủ dự án đối chiếu cơ quan thuế, sửa ngày
    và đặt `verified = true`; đếm được: 4/4 rule đã xác minh (hiện **0/4**);
  - mỗi vùng EU/FR/US/VN có ≥ 2 cơ quan trong `official.toml` đã bật và trả về văn bản thật;
  - mọi `summary` hiển thị kèm disclaimer; không có `summary` nào chứa nguyên văn `abstract`.
- **Chưa có**: connector văn bản theo vùng; nhắc Telegram/email (P7).
- **Rollback**: tắt rule/cơ quan (`enabled=0`) hoặc xoá file pack; không ảnh hưởng P0–P2.

### 5.5 P4 — chứng khoán, báo cáo tài chính và hồ sơ niêm yết (nền tảng filings đã làm 11/11/2026)

- **Bổ sung yêu cầu (11/11/2026)**: ngoài chỉ số/giá, P4 phải thu thập **báo cáo tài chính
  (quý/năm)**, **công bố khi niêm yết/IPO** (cáo bạch, hồ sơ đăng ký) và **công bố bất thường**.
- **Mô hình đã có** (migration 14): `companies` (CIK/mã ngoài, tên, ticker, sàn, vùng),
  `filings` (loại hồ sơ 10-K/10-Q/8-K/S-1…, số accession, kỳ báo cáo, ngày nộp, link gốc,
  `filing_kind` annual/quarterly/current/registration, tóm tắt tự viết + disclaimer),
  `financial_facts` (metric chuẩn hóa, concept gốc, kỳ, đơn vị, tiền tệ, `as_of`, `revision`,
  chất lượng). **Chỉ metadata + số liệu + link; không lưu toàn văn hồ sơ** (có test khẳng định).
- **Connector `sec-edgar-filings`** (đề xuất → đã viết, mặc định TẮT): submissions API +
  XBRL companyfacts; 2 request/công ty/lần; cần `SEC_USER_AGENT` do chủ dự án điền; test loopback.
- **Ứng viên nguồn** (ghi trong `.docs/sources-candidates.md` mục 7, mức kiểm chứng A/B/C):
  - Mỹ — SEC EDGAR: **A**. API công khai, 10 request/giây, cần User-Agent có tên + email; trang
    chính thức cho phép dùng lại kèm ghi nguồn.
  - EU/Pháp — AMF/ESMA/ESEF: **B/C**. AMF có BDIF/GECO/ONDE nhưng **không thấy API công khai**;
    ESAP chưa hoạt động (công khai "by July 2027"). **Cần chủ dự án chọn**: chờ ESAP, xin quyền
    AMF, hay chỉ lưu link gốc do người dùng thêm.
  - Việt Nam — UBCKNN/HOSE/HNX: **C**. Không có API chính thức được công bố; các trang lỗi khi
    truy cập tự động. Không cào.
- **Phân tích tài chính** (tỷ số, so sánh kỳ, nhận định) thuộc **P6**, tách rõ **số liệu đã xác
  minh** (có `as_of` + nguồn) với **suy luận**, và mọi kết quả kèm "không phải tư vấn đầu tư".
- **Nghiệm thu đo được**: ≥ 1 công ty Mỹ với ≥ 2 loại hồ sơ (báo cáo + đăng ký) và ≥ 4 metric
  có kỳ; mọi giá trị có nguồn + `as_of`; 0 trường hợp lưu toàn văn; EU/VN chỉ khi có quyết định
  của chủ dự án ở trên.
- **Chưa có**: connector AMF/ESEF và Việt Nam; phân tích tỷ số (P6).

### 5.5b P4 — chỉ số và giá (đề xuất, chưa làm)

- **Đầu vào**: P2 (kho series + connector) và P3 (kho documents cho công bố). Cần chủ dự án chọn
  nhà cung cấp giá (mục 8 câu 3) vì dữ liệu giá thời gian thực gần như luôn có phí/điều khoản.
- **Nghiệm thu đo được**: watchlist 3 vùng (US/FR/VN) hiển thị ≥ 1 chỉ số và ≥ 1 công bố mới mỗi
  vùng; mỗi giá trị có `attribution` + điều khoản ghi trong docs; 0 nguồn không có điều khoản.
- **Điều kiện dừng**: nếu chưa có nhà cung cấp giá hợp lệ, P4 dừng ở "công bố chính thức + chỉ số
  trễ" và ghi rõ phần giá bị bỏ.
- **Rollback**: tắt connector mới; dữ liệu nằm trong `series`/`documents` sẵn có.

### 5.6 P5 — phân loại, tóm tắt, gom story (nền tảng offline đã làm 11/11/2026)

- **Lớp provider dùng chung P5/P6** (`src/hub.rs`): một provider cho cả LLM và agent.
  - LLM qua API **OpenAI-compatible**: `base_url` + alias model **riêng cho từng tác vụ**
    (`classify` rẻ, `summarise` tầm trung, `analyse` mạnh), đổi alias không cần sửa mã.
  - **Agent** từ catalog Hub (mặc định `news-analyst-argus`) cho nghiên cứu: `POST /agents/{id}/run`,
    có timeout và trần độ dài đầu ra.
  - **Mỗi app một khóa**: secret `hub-api-key` trong kho mã hóa, nhập ở Cài đặt (ô chỉ ghi);
    không vào env/log/API/git. **Chưa từng nhập khóa hộ, chưa gọi Hub thật.**
  - **Allowlist host**: mặc định vẫn chặn địa chỉ riêng; chỉ host **được nêu đúng tên** trong
    `HUB_ALLOWED_HOSTS` mới được phân giải ra địa chỉ nội bộ. Quyền này gắn với bộ tải của Hub
    duy nhất; RSS/connector không bị ảnh hưởng.
  - Hạn mức token/ngày kiểm **trước** khi gọi, cache theo hash(model+tác vụ+prompt+nội dung),
    timeout, lỗi ⇒ caller giữ nhãn luật; chế độ không-LLM vẫn chạy.
  - **Kiểm tra kết nối** chỉ gọi liệt kê model, chỉ khi admin bấm, không tốn token.
  - Việc phía Hub admin (tạo khóa, quyền, alias) ở `.docs/hub-integration.md`.
- **Đã có (offline)**:

- **Đầu vào**: P1 (bài có tiêu đề/link), kho secret (khóa LLM), tùy chọn P1.5 (vùng).
- **Phần offline (đã làm 11/11/2026)**: phân loại bằng **luật** (vùng từ nguồn, chủ đề từ từ khóa, thực thể
  từ mẫu mã chứng khoán/tiền tệ/tổ chức) + lớp provider LLM qua Agent Hub/LiteLLM với provider
  **giả lập**, hạn mức token/ngày, cache theo hash nội dung, và chế độ "không LLM" vẫn chạy.
- **Nghiệm thu đo được** (phần offline): 50 mục mẫu tự dựng, **tách 25 dev / 25 holdout** để tránh
  đo trên chính dữ liệu đã tinh luật; chạy lại cùng input cho cùng output; vượt hạn mức token thì
  dừng và báo; chế độ không LLM cho ra nhãn luật. **Kết quả đo 11/11/2026: dev 24/25 = 96%,
  holdout 21/25 = 84%** (ngưỡng 80%). Danh sách mục sai được in khi chạy `cargo test -- --nocapture
  classify::tests::the_rule_classifier_is_measured_on_dev_and_holdout`.
- **Đã bổ sung 11/11/2026**: `/api/articles` trả `labels {topics, entities}` (một truy vấn mỗi trang) và người đọc thấy chip chủ đề + thực thể; việc "đưa nhãn thực thể lên UI" vì thế đã xong. Phần còn lại của P5 (tóm tắt bằng LLM, gom story) vẫn chưa làm.
- **Nghiệm thu phía thật** (chưa làm): cùng 50 mục nhưng qua LLM thật; đo tỉ lệ đúng, chi phí
  token/lần, và so với luật; ghi vào PROJECT_STATUS.
- **Rollback**: tắt LLM (chế độ luật) là hệ thống vẫn chạy; nhãn do con người sửa được.

### 5.7 P6 — phân tích, báo cáo, scorecard (đề xuất, chưa làm)

- **Đầu vào**: P5 (nhãn + tóm tắt) và P2 (số liệu). Phụ thuộc P5, không cần P4.
- **Nghiệm thu đo được**: một bản tin ngày có ≥ 5 mục, mỗi mục có link nguồn bấm được; mỗi nhận
  định tách rõ "sự kiện đã xác minh" và "suy luận"; scorecard chấm được ít nhất 1 dự báo khi đến
  hạn và hiển thị đúng/sai kèm lý do.
- **Rollback**: tắt chế độ phân tích; dữ liệu nguồn không đổi.

### 5.8 P7 — cảnh báo (nền tảng offline đã làm 11/11/2026)

- **Đầu vào**: P5 (nhãn) đã có; P6 (nội dung phân tích) và P2 (ngưỡng số liệu) chưa có.
- **Đã làm (offline, migration 15)**: `alert_rules` (khớp `region`/`topic`/`entity`/`keyword`,
  mức info/important/urgent, cooldown, giờ yên lặng có múi giờ), `alert_deliveries`
  **UNIQUE (rule, article)** nên một sự kiện chỉ báo đúng một lần kể cả khi quét lại nhiều lần,
  `alert_skip_log` ghi lý do không bắn (đang tắt / đã gửi / cooldown / giờ yên lặng) để màn hình
  cài đặt giải thích được sự im lặng. Quét nền bật ở lần đăng nhập admin đầu tiên; trước đó mọi
  luật đều tắt. Khớp bài dùng **cùng luật hiển thị** với người đọc (vùng bật + chủ đề bật) nên
  không bao giờ báo thứ người đọc không thấy. Bỏ qua vì giờ yên lặng **không** đánh dấu đã gửi,
  nên sự kiện vẫn bắn sau khi hết giờ yên lặng.
- **Kênh**: chỉ `in-app` tồn tại. `telegram`/`email`/`webhook`/`imap`/`slack` được nêu tên ở một
  chỗ và **bị từ chối** (`channel_not_implemented`) thay vì lưu rồi im lặng không gửi.
- **Nghiệm thu đo được (offline, có test)**: 16 test alert + 2 test mức router — quét hai lần chỉ
  tạo **1** thông báo; cooldown chặn rồi hết hạn; giờ yên lặng 22:00–07:00 qua nửa đêm không làm
  mất sự kiện; bài ở chủ đề tắt không bao giờ báo; `match_kind` lạ khớp **rỗng** chứ không khớp
  tất cả; API từ chối kênh chưa có. Kiểm tay trên domain thật: `sweep` hai lần → `created 1` rồi
  `created 0`.
- **Chưa làm**: gửi thật Telegram/email/webhook (kể cả ký HMAC), luật theo **ngưỡng số liệu**,
  gộp theo story, chuông trên thanh điều hướng (hiện là một mục trang riêng).
- **Nghiệm thu phía thật**: cần chủ dự án chọn kênh và cấp khóa; chưa làm.
- **Rollback**: tắt từng luật hoặc lật cờ enabled là đủ; bảng cảnh báo không ảnh hưởng dữ liệu tin.

### 5.9 P8 — API/MCP/Hub (đề xuất, chưa làm)

- **Đầu vào**: P4–P7 (nguồn dữ liệu cho từng endpoint).
- **Nghiệm thu đo được**: gọi được `newsman__market_snapshot` và `newsman__region_brief` qua Hub
  với đúng 1 khóa; mọi endpoint v1 trả 401 khi không có khóa; OpenAPI/API_DOCS khớp 100% route.
- **Rollback**: gỡ connector khỏi Hub; REST v1 không phụ thuộc MCP.

### 5.10 P9 — phát hành (đề xuất, chưa làm)

- **Đầu vào**: các giai đoạn trong phạm vi phát hành + duyệt từng bước của chủ dự án.
- **Nghiệm thu đo được**: stack `lnc-newsman` healthy trên `10.20.2.1`; `/health` 200 qua domain;
  cổng host chốt (hiện `8462` là **đề xuất**); `DATA_DIR` đúng `/share/Docker`; khôi phục thử một
  bản sao lưu vào môi trường tạm thành công.
- **Rollback**: giữ image trước đó, `docker compose` về tag cũ; dữ liệu trong `DATA_DIR` không bị
  migration xoá (mọi migration chỉ thêm bảng/cột).

## 6. Rủi ro

| Rủi ro | Xử lý |
|---|---|
| Giấy phép dữ liệu thị trường (giá thời gian thực thường có phí, cấm phân phối lại) | Ưu tiên công bố chính thức và gói có điều khoản rõ; API/MCP chỉ trả dữ liệu được phép |
| Dữ liệu chứng khoán Việt Nam thiếu API chính thức ổn định | Bắt đầu bằng công bố thông tin; chọn nhà cung cấp sau khi anh duyệt |
| Nhận định tài chính bị hiểu là khuyến nghị đầu tư | Ghi rõ ranh giới; tách sự kiện và suy luận; scorecard minh bạch độ chính xác |
| Tóm tắt văn bản thuế sai | Luôn dẫn văn bản gốc, ghi ngày hiệu lực; không thay tư vấn thuế |
| Region pack lỗi cú pháp làm hỏng job | Kiểm tra từng file, bỏ qua file lỗi và báo trong UI; có test |
| Khóa API nhà cung cấp lộ | Kho secret mã hóa trước khi bật connector; không ghi khóa vào log/API |
| Nguồn chặn bot | Bậc thang fetch; ưu tiên RSS/API chính thức |
| Chi phí LLM | Luật chạy trước; hạn mức; cache |
| Chưa rõ CPU NAS | Kiểm tra trước P9 |
| Business MCP Hub lỗi phiên | Chấm cổng chất lượng plan khi Hub ổn |

## 7. Ngoài phạm vi

Đặt lệnh giao dịch hay kết nối tài khoản môi giới, khuyến nghị đầu tư cá nhân, tư vấn pháp lý/thuế, lưu toàn văn bài báo, ứng dụng di động riêng, web push, SSO Keycloak.

## 8. Quyết định: đã chốt và còn mở

### Đã chốt (chủ dự án quyết)

| # | Nội dung | Ngày |
|---|---|---|
| 1 | Stack Rust + axum + SQLite + React/Vite; dựng P0 | 11/11/2026 |
| 2 | Phạm vi đa lĩnh vực (tài chính, BTC/on-chain, chứng khoán, thế giới, văn bản/thuế) | 11/11/2026 |
| 3 | Vùng FR (cha EU), US, VN; `global` là gốc ảo của pack | 11/11/2026 |
| 4 | Region pack dạng thư mục; phát hiện ≠ kích hoạt | 11/11/2026 |
| 5 | 6 connector nguồn theo `.docs/sources-candidates.md` mục 6 | 11/11/2026 |
| 6 | Chấp nhận P1.5, P2 (107 test), P3 (124 test) ở mức nền tảng + test giả lập | 11/11/2026 |

### Còn mở (cần chủ dự án quyết trước giai đoạn tương ứng)

| # | Câu hỏi | Chặn giai đoạn | Mặc định nếu chưa quyết |
|---|---|---|---|
| 1 | Duyệt tải feed thật + danh sách ~20 nguồn cho P1 | P1 nghiệm thu | Không tải; P1 dừng ở test giả lập |
| 2 | Duyệt chạy `.docs/live-acceptance.md` cho P2 | P2 nghiệm thu | Không gọi nhà cung cấp thật |
| 3 | Xác minh hạn nộp thuế từng vùng (hiện 0/4 rule `verified`) | P3 nghiệm thu | Hạn nộp là dữ liệu mẫu, không dùng thật |
| 4 | Nhà cung cấp giá chứng khoán: chỉ nguồn miễn phí/chính thức, hay trả phí có khóa | P4 | Chỉ công bố chính thức + chỉ số trễ |
| 5 | Khóa LLM: dùng Agent Hub hay LiteLLM trực tiếp; ngân sách token/ngày | P5 phần thật | Chỉ chạy chế độ luật (không LLM) |
| 6 | Bitcoin: API công khai hay node Bitcoin Core riêng | P2 mở rộng | Chỉ API công khai (đã làm) |
| 7 | Kênh cảnh báo nào bật trước (chuông/Telegram/email/webhook) | P7 | Chỉ chuông trong app |
| 8 | Cổng host `8462` và tên miền có giữ không | P9 | Giữ đề xuất, không deploy |

### Đề xuất chưa duyệt (không chặn gì)

- Chạy scheduler cho connector (hiện chỉ admin bấm) — đề xuất làm ở P7 cùng cảnh báo sức khỏe.
- Biểu đồ và trang Thị trường đầy đủ — đề xuất gộp vào P4.
- Kho sách truyện/ebook thành app riêng — xem mục 9.

## 9. Dự án liên quan (đề xuất, chưa duyệt)

Kho sách truyện/ebook/sách nói: đề xuất làm **app riêng có kết nối** với Newsman, không gộp. Xem [library/research.md](library/research.md).
