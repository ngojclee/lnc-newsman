# Ứng viên nguồn dữ liệu miễn phí (11/11/2026)

Trạng thái: **đề xuất để anh chọn. Chưa gọi thử nguồn nào, chưa viết connector thật.**
Cách tra: tìm kiếm web và đọc kết quả. Cột "Mức kiểm chứng" cho biết điều gì đã thấy trong kết quả, điều gì chưa. Mọi điều khoản, hạn mức, URL phải được đọc lại trên trang chính thức của nhà cung cấp trước khi bật connector.

Quy ước mức: **A** = tìm thấy ở trang chính thức hoặc nhiều nguồn khớp; **B** = chỉ thấy ở nguồn thứ cấp; **C** = chưa tra được.

## 1. Bitcoin, on-chain và giá

| Nguồn | Dùng cho | Chi phí/khóa | Điều đã thấy | Mức |
|---|---|---|---|---|
| mempool.space API công khai | phí, mempool, khối, hashrate, độ khó | miễn phí, không khóa | Tài liệu chính thức nói có giới hạn tốc độ, vượt thì HTTP 429, vi phạm lặp lại có thể bị cấm; không công bố con số; có gói enterprise. Mã mở AGPL-3.0, tự dựng được | A (giới hạn số: C) |
| mempool.space tự dựng | như trên, không giới hạn | cần node Bitcoin + indexer | Tốn tài nguyên NAS; để sau | B |
| CoinGecko gói Demo | giá BTC theo USD/EUR/VND | miễn phí, cần khóa | Trang giá chính thức: 100 lần/phút, 10.000 lần/tháng, **bắt buộc ghi nguồn**; không được bán lại hay phân phối lại API; các tài liệu khác ghi 30 hoặc 50 lần/phút nên cần xác nhận; chưa đọc Điều khoản và hướng dẫn ghi nguồn | A (số liệu: B) |
| Farside, bảng dòng tiền ETF BTC | dòng tiền ETF | miễn phí | Bảng web, số liệu ước tính và tạm thời, có ô trống; không thấy API; chưa đọc điều khoản | B |
| Blockstream Esplora, blockchain.com charts | phương án dự phòng | — | Chưa tra được | C |

Gợi ý: bắt đầu bằng mempool.space (không khóa, nhẹ), giá BTC từ CoinGecko Demo. Dòng tiền ETF chỉ nên làm sau khi đọc điều khoản Farside hoặc lấy từ công bố của nhà phát hành.

## 2. Vĩ mô

| Nguồn | Vùng | Chi phí/khóa | Điều đã thấy | Mức |
|---|---|---|---|---|
| FRED | Mỹ | miễn phí, cần tài khoản và khóa | Điều khoản chính thức yêu cầu đăng ký khóa; cần ghi FRED và nguồn gốc dữ liệu; điều khoản có thể đổi không báo trước | A |
| ECB Data Portal (SDMX) | EU | miễn phí, không khóa | Thấy ở tài liệu ECB và nguồn thứ cấp; không công bố giới hạn tốc độ; nguồn thứ cấp nói được dùng lại nếu ghi "Source: ECB statistics" và không sửa dữ liệu, chưa thấy ở trang ECB | A (điều khoản dùng lại: B) |
| Eurostat, INSEE, Tổng cục Thống kê, NHNN | EU, Pháp, Việt Nam | — | Chưa tra được | C |

## 3. Văn bản chính thức và thuế

| Nguồn | Vùng | Chi phí/khóa | Điều đã thấy | Mức |
|---|---|---|---|---|
| Federal Register API | Mỹ | miễn phí, không khóa | Tài liệu chính thức nói không cần khóa; mỗi truy vấn tối đa 10.000 văn bản; tên bộ lọc và trường trả về có chỗ khác nhau | A |
| Légifrance (PISTE) | Pháp | miễn phí, phải đăng ký, chấp nhận điều khoản PISTE và API | Có hạn mức (con số 2.000 lần/ngày chỉ thấy ở nguồn thứ ba); dữ liệu hàng loạt tải tự do qua `echanges.dila.gouv.fr/OPENDATA`; giấy phép mở Etalab 2.0 | A (hạn mức: C) |
| EUR-Lex RSS | EU | miễn phí | Nguồn cấp theo tài khoản đã đăng ký; có URL công khai dạng `display-feed.rss?rssId=`, một số ID đã ngừng; nguồn thứ cấp nói dữ liệu được dùng lại nếu ghi nguồn | B |
| BOFiP, impots.gouv, IRS, Treasury, Fed, Ủy ban châu Âu, Công báo, Bộ Tài chính, Cục Thuế | — | — | Chưa tra được | C |

## 4. Chứng khoán

| Nguồn | Vùng | Chi phí/khóa | Điều đã thấy | Mức |
|---|---|---|---|---|
| SEC EDGAR | Mỹ | miễn phí, không khóa | Trang chính thức: tối đa 10 yêu cầu/giây, phải khai báo User-Agent có tên và email liên hệ; thiếu thì bị chặn; vượt thì bị chặn tạm thời | A |
| Giá chứng khoán Pháp/EU (Euronext, AMF) | Pháp/EU | — | Chưa tra được | C |
| Việt Nam: HOSE, HNX, UBCKNN | Việt Nam | — | Chưa tìm được API chính thức công khai. Có VietstockFinance (gói cơ bản miễn phí nhưng giới hạn), SSI (cần token), và thư viện cộng đồng `vnstock` tự nhận dùng API công khai của bên thứ ba, không chính thức nên cần kiểm tra ổn định và điều khoản | B |

Giá chứng khoán thời gian thực gần như không có nguồn miễn phí hợp lệ để hiển thị lại. Với giai đoạn đầu nên chỉ làm công bố chính thức (EDGAR, AMF, HOSE/HNX) và chỉ số trễ nếu điều khoản cho phép.

## 5. Tin tức (RSS)

| Nguồn | Vùng | Điều đã thấy | Mức |
|---|---|---|---|
| VnExpress | Việt Nam | Có feed như `vnexpress.net/rss/kinh-doanh.rss`; điều khoản ghi miễn phí cho cá nhân và tổ chức phi lợi nhuận, phải nêu rõ nguồn | B |
| VnEconomy, Thanh Niên, VTV | Việt Nam | Có URL feed trong mã mẫu (chứng khoán, kinh tế); chưa kiểm điều khoản | B |
| BBC Business | Quốc tế | URL feed từ danh sách 2020, cần kiểm lại; chưa đọc điều khoản dùng cá nhân | B |
| Reuters | Quốc tế | **Đã ngừng RSS chính thức từ 06/2020**; các feed thay thế qua Google News là không chính thức | A |
| CafeF, Vietstock, Les Echos, Le Monde, France Info, AP | — | Chưa xác nhận URL feed hay điều khoản | C |

## 6. Đề xuất bộ nguồn đầu tiên

Bộ nhỏ, không phải ngân sách, đủ để thử connector thật mà ít rủi ro điều khoản:

1. mempool.space (Bitcoin, không khóa).
2. ECB Data Portal (vĩ mô EU, không khóa) và FRED (vĩ mô Mỹ, một khóa miễn phí).
3. Federal Register và SEC EDGAR (Mỹ, không khóa; EDGAR cần User-Agent có email liên hệ, nên để anh chọn email dùng).
4. Légifrance qua dữ liệu mở hàng loạt hoặc PISTE (cần đăng ký).
5. RSS VnExpress và vài báo Việt Nam khác sau khi kiểm điều khoản.

Việc cần anh làm: chọn bộ nguồn, quyết có tạo tài khoản FRED/CoinGecko/PISTE (tôi không tự đăng ký tài khoản hay nhập khóa), chọn email liên hệ cho User-Agent, và cho phép tải thử. Sau đó mới viết connector thật và chạy nghiệm thu.

## Nguồn tra cứu

- https://api.mempool.space/docs/api · https://www.coingecko.com/en/api/pricing · https://farside.co.uk/?p=997
- https://fred.stlouisfed.org/docs/api/terms_of_use.html · https://data.ecb.europa.eu/help/api/overview
- https://www.federalregister.gov/reader-aids/developer-resources/rest-api · https://www.sec.gov/os/accessing-edgar-data
- https://www.data.gouv.fr/dataservices/legifrance/ · https://eur-lex.europa.eu/content/help/my-eurlex/my-rss-feeds.html
- https://www.fivefilters.org/category/feed-control/page/2/ (Reuters ngừng RSS)

## 7. Báo cáo tài chính, hồ sơ niêm yết/IPO và công bố bất thường (bổ sung 11/11/2026)

Đề xuất cho P4; **chưa kiểm chứng điều khoản với từng nhà cung cấp** trừ nơi ghi rõ. Đã đọc trang chính thức khi có thể (mức A), chỉ thấy ở nguồn thứ cấp (B), hoặc chưa tra được (C).

| Nguồn | Vùng | Dùng cho | Truy cập | Điều khoản đã thấy | Mức |
|---|---|---|---|---|---|
| SEC EDGAR — submissions API `data.sec.gov/submissions/CIK##########.json` | Mỹ | danh sách hồ sơ (10-K, 10-Q, 8-K, S-1…), tên công ty, ticker, sàn | JSON, không cần khóa | Trang chính thức: tối đa **10 yêu cầu/giây**, phải khai báo User-Agent có tên + email; nội dung trên sec.gov "may be copied or further distributed … Please consider appropriate citation to the SEC as the source"; tên SEC và logo EDGAR là nhãn hiệu, không dùng làm tên thương mại/tên miền | **A** |
| SEC EDGAR — XBRL company facts `data.sec.gov/api/xbrl/companyfacts/CIK##########.json` | Mỹ | số liệu có cấu trúc (doanh thu, lợi nhuận, tài sản, nợ) theo taxonomy us-gaap/ifrs-full/dei | JSON, không cần khóa | như trên; bao phủ 10-Q, 10-K, 8-K, 20-F, 40-F, 6-K (trang không nêu S-1) | **A** |
| SEC EDGAR — bulk `companyfacts.zip`, `submissions.zip` | Mỹ | tải toàn bộ một lần | ZIP, dựng lại hằng đêm ~03:00 ET | như trên | **A** |
| AMF (Pháp) — BDIF/GECO/ONDE | Pháp | thông tin điều tiết, báo cáo thường niên, công bố bất thường | Trang web + RSS + extranet; **không thấy API công khai hay tải hàng loạt** trên trang đã đọc | Không thấy tuyên bố dùng lại/giấy phép trên trang đã đọc | **B** (tồn tại) / **C** (điều khoản) |
| ESMA — ESAP | EU | điểm truy cập chung cho thông tin tài chính/bền vững | **chưa hoạt động**: thu thập từ 7/2026, công khai "by July 2027", giai đoạn 2 từ 1/2028; không nêu API | Chưa đọc legal notice của ESMA | **A** (lộ trình) / **C** (điều khoản) |
| ESEF/XBRL Pháp | EU/Pháp | báo cáo hợp nhất định dạng XBRL | Chưa xác định nơi truy cập chính thức | — | **C** |
| UBCKNN / HOSE / HNX | Việt Nam | công bố thông tin, báo cáo tài chính, hồ sơ niêm yết | Không có API chính thức được công bố; hsx.vn trả trang gần như rỗng, hnx.vn lỗi TLS, ssc.gov.vn lỗi chuyển hướng khi truy cập tự động | Không tìm thấy | **C** |

**Kết luận đề xuất:** chỉ EDGAR có đường truy cập chính thức, có cấu trúc, và điều khoản cho phép dùng lại kèm ghi nguồn (mức A). AMF/ESEF và Việt Nam cần chủ dự án quyết định: (a) chờ ESAP 7/2027, (b) xin quyền/tài khoản với AMF, hay (c) chỉ lưu link gốc do người dùng thêm tay. **Không cào** trang không có điều khoản rõ.

## Nguồn tra cứu (bổ sung)

- https://www.sec.gov/edgar/sec-api-documentation · https://www.sec.gov/os/accessing-edgar-data · https://www.sec.gov/privacy.htm
- https://www.amf-france.org/fr/formulaires-et-declarations/societes-cotees-et-operations-financieres/diffusion-information-reglementee-0
- https://www.esma.europa.eu/esmas-activities/data/european-single-access-point-esap
- https://www.hsx.vn/ · https://www.hnx.vn/ · https://www.ssc.gov.vn/ (không truy cập được nội dung khi kiểm 11/11/2026)
