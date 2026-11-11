# Nghiệm thu trực tiếp với nhà cung cấp thật — kịch bản chờ duyệt

Trạng thái: **CHƯA CHẠY. Chờ chủ dự án duyệt riêng.** Cho tới khi duyệt, code chỉ chạy
với máy chủ giả lập loopback và **không có** lệnh nào trong tài liệu này được tự động hoá.

## Nguyên tắc chung

- Không đăng ký tài khoản, không tạo khóa, không điền email hộ. Những việc đó do chủ dự án
  làm; người thực hiện chỉ dùng giá trị đã có trong kho secret hoặc biến môi trường.
- Mỗi nguồn chạy **một lần**, số yêu cầu ghi rõ dưới đây. Nếu một bước vượt số đó, dừng.
- Dừng ngay khi thấy: HTTP 401/403/429, nội dung khác hình dạng tài liệu, hoặc bất kỳ dấu
  hiệu cần đăng nhập/trả phí.
- Sau mỗi bước, ghi lại: thời điểm, số yêu cầu, HTTP status, số bản ghi mới, thông báo lỗi
  (nếu có). Không ghi nội dung khóa hay email ra ngoài file local.
- Cổng chất lượng: không bước nào được coi là "đã nghiệm thu" nếu thiếu log + số liệu.

## Chuẩn bị (một lần, trước mọi bước)

| Việc | Ai làm | Ghi chú |
|---|---|---|
| Đọc lại điều khoản của từng nhà cung cấp (bảng dưới) | Chủ dự án | Không bỏ qua. Điều khoản có thể đã đổi so với 11/11/2026. |
| Đặt `LNC_MASTER_KEY` trên máy chạy nghiệm thu | Chủ dự án | 64 ký tự hex. Không dùng khóa demo. |
| Tạo khóa FRED và CoinGecko Demo (nếu định thử) | Chủ dự án | Nhập vào **Cài đặt → Khóa nhà cung cấp**. Người thực hiện không nhập hộ, không đọc lại. |
| Điền `SEC_USER_AGENT` (tên + email liên hệ) | Chủ dự án | Người thực hiện **không** tự chọn email. Để trống ⇒ connector EDGAR không chạy (đúng thiết kế). |
| Bật vùng tương ứng (us / eu / global) | Chủ dự án | Bật trong **Cài đặt → Vùng**. |
| Bật từng connector | Chủ dự án | Connector mặc định **tắt**; mọi lần chạy đều phải do người bấm. |

## Bước 1 — mempool.space (không khóa) · **4 yêu cầu**

Điều khoản cần đọc trước: <https://mempool.space/docs/api> (mục giới hạn tốc độ; mục về
việc vượt giới hạn trả HTTP 429 và có thể bị cấm).

- Bật vùng `global`, bật connector `mempool-space`.
- Bấm chạy **một lần**.
- Yêu cầu dự kiến: `/api/v1/fees/recommended`, `/api/mempool`,
  `/api/v1/mining/hashrate/3d`, `/api/v1/blocks/tip/height` → **4**.
- Đạt khi: cả 4 trả 200; có 8 chuỗi mới; một điểm/chuỗi; `attribution = "Source: mempool.space"`;
  giá trị khớp trang web mempool.space tại thời điểm chạy (sai số do thời điểm là bình thường).
- Dừng khi: 429 (không thử lại trong ngày), hoặc bất kỳ trường nào thiếu.

## Bước 2 — ECB Data Portal (không khóa) · **1 yêu cầu**

Điều khoản cần đọc trước: <https://data.ecb.europa.eu/help/api/overview> và trang điều kiện
sử dụng lại dữ liệu ECB. Dòng ghi nguồn bắt buộc: `Source: ECB statistics`.

- Bật vùng `eu`, bật connector `ecb-sdmx`.
- Chạy một lần → **1** yêu cầu (`/service/data/EXR/D.USD.EUR.SP00.A`).
- Đạt khi: 200; ≥ 20 điểm cho `ECB-EUR-USD`; giá trị khớp tỷ giá tham chiếu EUR/USD công bố;
  không làm tròn, không đổi đơn vị; `attribution` đúng chuỗi trên.
- Dừng khi: lỗi định dạng SDMX, hoặc thiếu giá trị ở giữa (lỗ hổng phải để trống, không nội suy).

## Bước 3 — Federal Register (không khóa) · **2 yêu cầu**

Điều khoản cần đọc trước: <https://www.federalregister.gov/reader-aids/developer-resources/rest-api>
(giới hạn 10.000 văn bản/truy vấn; khuyến nghị gửi User-Agent nhận diện).

- Bật vùng `us`, bật connector `federal-register`.
- Chạy một lần → tối đa **2** yêu cầu (trang 1, và trang 2 chỉ khi trang 1 đầy).
- Đạt khi: văn bản mới xuất hiện ở **Văn bản** với số hiệu, tiêu đề, `effective_on` (trường
  trả về) khớp bộ lọc `effective_date` đã gửi; `summary` **rỗng** (chưa có bản tóm tắt tự viết);
  không có câu chữ nào của `abstract` trong DB.
- Dừng khi: `count > 10000` (connector tự từ chối), hoặc trường `html_url` thiếu.

## Bước 4 — FRED (cần khóa) · **1 yêu cầu** — tùy chọn

Điều khoản cần đọc trước: <https://fred.stlouisfed.org/docs/api/terms_of_use.html>
(bắt buộc ghi FRED và nguồn gốc dữ liệu; điều khoản có thể đổi không báo trước).

- Chủ dự án nhập `fred-api-key` vào kho secret.
- Bật vùng `us`, bật connector `fred`; chạy một lần → **1** yêu cầu (`FEDFUNDS`).
- Đạt khi: ≥ 20 điểm; `attribution` = `Source: Federal Reserve Bank of St. Louis (FRED)`;
  khóa **không** xuất hiện trong DB, log hay API (kiểm bằng tìm chuỗi trong log server).
- Dừng khi: 400/401 (khóa sai — không thử lại), hoặc value `"."` bị ghi thành 0.

## Bước 5 — CoinGecko Demo (cần khóa) · **1 yêu cầu** — tùy chọn

Điều khoản cần đọc trước: <https://www.coingecko.com/en/api/pricing> và Điều khoản dịch vụ
CoinGecko (ghi nguồn bắt buộc; cấm bán lại/phân phối lại; giới hạn tốc độ).

- Chủ dự án nhập `coingecko-api-key`; connector gửi khóa qua **header**, không qua URL.
- Bật vùng bất kỳ đang bật, bật connector `coingecko`; chạy một lần → **1** yêu cầu.
- Đạt khi: 3 chuỗi BTC/USD, EUR, VND; `attribution` chứa "CoinGecko"; `requests_today ≤ 2`;
  không có 429.
- Dừng khi: 429 hoặc 401. Không chạy lặp để "thử lại" — chờ sang phút sau.

## Bước 6 — SEC EDGAR (không khóa, cần `SEC_USER_AGENT`) · **1 yêu cầu**

Điều khoản cần đọc trước: <https://www.sec.gov/os/accessing-edgar-data> (≤ 10 yêu cầu/giây;
bắt buộc User-Agent có tên + email liên hệ).

- Chủ dự án điền `SEC_USER_AGENT="Tên tổ chức email@domain"` rồi khởi động lại app.
- Bật vùng `us`, bật connector `sec-edgar`; chạy một lần → **1** yêu cầu.
- Đạt khi: 200; ghi chú connector hiển thị số filing đọc được; không lưu nội dung.
- Dừng khi: 403 (SEC coi User-Agent không hợp lệ — sửa cấu hình, không thử lại liên tục).
- Nếu `SEC_USER_AGENT` trống: connector phải hiện `connector_not_configured` và **không**
  gửi yêu cầu nào. Kiểm điều này trước khi điền giá trị.

## Bước 7 — RSS Việt Nam (chỉ RSS, chưa bật)

- Không chạy trong đợt nghiệm thu connector. Nguồn `vnexpress-kinh-doanh` nằm trong pack
  `vn` ở trạng thái **tắt**; chỉ bật sau khi chủ dự án đọc lại điều khoản VnExpress.
- Khi bật: một lần quét = 1 yêu cầu; chỉ lưu tiêu đề/link/ngày; hiển thị kèm "Nguồn: VnExpress".

## Sau khi chạy

Ghi vào `.docs/PROJECT_STATUS.md`: nguồn nào đã nghiệm thu, ngày, số yêu cầu thực tế, kết quả.
Những nguồn chưa chạy vẫn giữ nhãn "đã test giả lập, chưa nghiệm thu".

## Nguồn chưa làm và lý do

| Nguồn | Lý do |
|---|---|
| Légifrance / PISTE | Cần đăng ký tài khoản và chấp nhận điều khoản PISTE; chưa có khóa. |
| Chứng khoán VN (HOSE/HNX) | Chưa có API chính thức công khai ổn định. |
| Chứng khoán Pháp/EU (Euronext, AMF) | Chưa xác nhận điều khoản và endpoint. |
| ETF Farside | Bảng web, không có API; số liệu ước tính; chưa đọc điều khoản. |
