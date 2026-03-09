# Phase Implementation Map

PRD(v1.8)에 정의된 Phase 0~8의 각 요구사항이 코드베이스의 어디에 구현되었는지를 매핑한 문서.

> 기준 코드: 2026-03-09 시점의 `main` 브랜치

---

## Phase 0: Core Analyze Foundation

> Rust core에서 workspace/ZIP 읽기, 엔트리 탐지, 상대경로 에셋 해석, 진단 JSON 생성

### P0-1. 모델 정의 (ProjectIndex, EntryCandidate, AssetRef, Diagnostic, Error)

| 모델 | 파일 | 위치 |
|------|------|------|
| `ProjectIndex` | `crates/marknest-core/src/lib.rs` | L79-87 |
| `EntryCandidate` | `crates/marknest-core/src/lib.rs` | L39-42 |
| `AssetRef` | `crates/marknest-core/src/lib.rs` | L60-69 |
| `AssetReferenceKind` (MarkdownImage, RawHtmlImage) | `crates/marknest-core/src/lib.rs` | L44-50 |
| `AssetStatus` (Resolved, Missing, External, UnsupportedScheme) | `crates/marknest-core/src/lib.rs` | L51-58 |
| `Diagnostic` | `crates/marknest-core/src/lib.rs` | L71-77 |
| `AnalyzeError` | `crates/marknest-core/src/lib.rs` | L153-172 |
| `RenderHtmlError` | `crates/marknest-core/src/lib.rs` | L180-207 |
| `ProjectSourceKind` (Workspace, Zip) | `crates/marknest-core/src/lib.rs` | L22-28 |
| `EntrySelectionReason` | `crates/marknest-core/src/lib.rs` | L29-37 |

### P0-2. 파일 시스템 추상화 (메모리 FS / 실제 FS)

| 구현 | 파일 | 위치 |
|------|------|------|
| `IndexedFileSystem` trait | `crates/marknest-core/src/lib.rs` | L322-332 |
| `IndexedFile` struct | `crates/marknest-core/src/lib.rs` | L334-338 |
| `WorkspaceFileSystem` (실제 FS) | `crates/marknest-core/src/lib.rs` | L340-371 |
| `ZipMemoryFileSystem` (메모리 FS) | `crates/marknest-core/src/lib.rs` | L373-447 |

### P0-3. 경로 정규화, allowlist 검사, Zip Slip 차단

| 구현 | 파일 | 위치 |
|------|------|------|
| `normalize_path()` | `crates/marknest-core/src/lib.rs` | L739-757 |
| `normalize_relative_string()` | `crates/marknest-core/src/lib.rs` | L759-792 |
| `has_windows_drive_prefix()` | `crates/marknest-core/src/lib.rs` | L1001-1004 |
| ZIP 엔트리 필터링 (Zip Slip 차단) | `crates/marknest-core/src/lib.rs` | L393-431 |
| Workspace 스캔 시 경로 검증 | `crates/marknest-core/src/lib.rs` | L520-530 |
| `MAX_ZIP_FILE_COUNT = 4_096` | `crates/marknest-core/src/lib.rs` | L12 |
| `MAX_ZIP_UNCOMPRESSED_BYTES = 256 MB` | `crates/marknest-core/src/lib.rs` | L13 |

### P0-4. Workspace 스캐너

| 구현 | 파일 | 위치 |
|------|------|------|
| `analyze_workspace()` | `crates/marknest-core/src/lib.rs` | L264-266 |
| `collect_workspace_files()` (재귀 디렉터리 탐색) | `crates/marknest-core/src/lib.rs` | L489-541 |
| `should_skip_directory()` (.git, target, node_modules, __MACOSX) | `crates/marknest-core/src/lib.rs` | L543-545 |
| `is_markdown_path()` (.md, .markdown) | `crates/marknest-core/src/lib.rs` | L962-965 |
| `is_supported_image_path()` (png, jpg, gif, svg, webp, bmp, avif) | `crates/marknest-core/src/lib.rs` | L967-974 |

### P0-5. ZIP 분석기

| 구현 | 파일 | 위치 |
|------|------|------|
| `analyze_zip()` | `crates/marknest-core/src/lib.rs` | L268-270 |
| `ZipMemoryFileSystem::new()` (안전한 ZIP 파싱) | `crates/marknest-core/src/lib.rs` | L377-437 |
| 파일 수 제한 (4,096) | `crates/marknest-core/src/lib.rs` | L385-391 |
| 압축 해제 크기 제한 (256 MB) | `crates/marknest-core/src/lib.rs` | L408-421 |
| 디렉터리 스킵, 경로 정규화 | `crates/marknest-core/src/lib.rs` | L393-406 |

### P0-6. 엔트리 탐지 규칙

| 구현 | 파일 | 위치 |
|------|------|------|
| `select_entry()` | `crates/marknest-core/src/lib.rs` | L547-602 |

탐지 우선순위:
1. 마크다운 파일 없음 → `NoMarkdownFiles` (L548-550)
2. 루트 `README.md` → `Readme` (L552-557)
3. 하위 디렉터리 단일 README.md → `Readme` (L559-571)
4. 루트 `index.md` → `Index` (L573-578)
5. 하위 디렉터리 단일 index.md → `Index` (L580-592)
6. 마크다운 1개 → `SingleMarkdownFile` (L594-599)
7. 다중 후보 → `MultipleCandidates` (L601)

### P0-7. Markdown 이미지 및 raw HTML img 에셋 해석기

| 구현 | 파일 | 위치 |
|------|------|------|
| `collect_assets()` | `crates/marknest-core/src/lib.rs` | L604-636 |
| `extract_markdown_image_destinations()` | `crates/marknest-core/src/lib.rs` | L801-827 |
| `extract_raw_html_img_sources()` | `crates/marknest-core/src/lib.rs` | L872-893 |
| `extract_src_attribute()` | `crates/marknest-core/src/lib.rs` | L895-937 |
| `resolve_asset_reference()` | `crates/marknest-core/src/lib.rs` | L638-727 |
| `resolve_local_asset_path()` | `crates/marknest-core/src/lib.rs` | L729-737 |
| `is_external_reference()` | `crates/marknest-core/src/lib.rs` | L981-984 |
| `remote_fetch_url()` | `crates/marknest-core/src/lib.rs` | L272-278 |
| `normalize_github_repository_image_url()` | `crates/marknest-core/src/lib.rs` | L284-320 |
| `join_with_entry_directory()` | `crates/marknest-core/src/lib.rs` | L950-956 |
| `strip_reference_query_and_fragment()` | `crates/marknest-core/src/lib.rs` | L794-799 |

### P0-8. JSON 직렬화 및 fixture 기반 golden 테스트

| 구현 | 파일 | 위치 |
|------|------|------|
| 모든 모델에 `#[derive(Serialize)]` | `crates/marknest-core/src/lib.rs` | L22-87 |
| Golden test JSON | `crates/marknest-core/tests/golden/workspace_valid.json` | - |
| Workspace 분석 테스트 | `crates/marknest-core/tests/analyze_workspace.rs` | L14-97+ |
| ZIP 분석 테스트 | `crates/marknest-core/tests/analyze_zip.rs` | - |

테스트 fixture 디렉터리 (`crates/marknest-core/tests/fixtures/`):
- `workspace_valid/` - 정상 workspace + 해석된 에셋
- `workspace_missing_asset/` - 누락 이미지 감지
- `workspace_multiple_entries/` - 다중 MD 후보 감지
- `workspace_asset_query_suffix/` - 쿼리 파라미터 처리
- `workspace_root_relative_asset/` - 루트 상대경로 해석
- `workspace_remote_http_assets/` - 외부 URL 처리
- `workspace_raw_html_sanitize/` - HTML 새니타이즈
- `workspace_render_features/` - 렌더링 기능
- `workspace_toc/` - 목차
- `workspace_emoji_shortcodes/` - 이모지
- `workspace_image_layout/` - 이미지 레이아웃
- `workspace_long_code_line/` - 코드 블록
- `workspace_collapsed_details/` - details/summary 요소

---

## Phase 1: CLI Validate

> `validate` 명령으로 `.md`, `.zip`, 폴더 입력 검증

### P1-1. CLI 명령 구조 스캐폴딩

| 구현 | 파일 | 위치 |
|------|------|------|
| `run()` (메인 진입점) | `crates/marknest/src/lib.rs` | L39 |
| `parse_cli()` (명령 라우터) | `crates/marknest/src/lib.rs` | L1909 |
| 서브커맨드 분기 (help, validate, convert) | `crates/marknest/src/lib.rs` | L1923-1925 |
| `ParseResult` enum | `crates/marknest/src/lib.rs` | L3600 |

### P1-2. 입력 타입 판별기

| 구현 | 파일 | 위치 |
|------|------|------|
| `resolve_input()` | `crates/marknest/src/lib.rs` | L835 |
| `ResolvedInput` enum (MarkdownFile, Folder, Zip) | `crates/marknest/src/lib.rs` | L2344 |
| `is_markdown_path()` (.md, .markdown 확장자) | `crates/marknest/src/lib.rs` | L1888 |
| `is_zip_path()` (.zip 확장자) | `crates/marknest/src/lib.rs` | L1897 |
| 미지정 시 현재 경로 사용 | `crates/marknest/src/lib.rs` | L836 |

### P1-3. validate와 core 분석기 연결

| 구현 | 파일 | 위치 |
|------|------|------|
| `run_validate()` | `crates/marknest/src/lib.rs` | L72 |
| `parse_validate_args()` | `crates/marknest/src/lib.rs` | L1933 |
| `ValidateArgs` (input, entry, all, strict, report) | `crates/marknest/src/lib.rs` | L2226 |
| `analyze_input()` → core 호출 | `crates/marknest/src/lib.rs` | L724 |
| `determine_selection()` (--entry, --all, auto) | `crates/marknest/src/lib.rs` | L901 |
| `filter_diagnostics()` | `crates/marknest/src/lib.rs` | L1006 |

### P1-4. 진단 포맷터

| 구현 | 파일 | 위치 |
|------|------|------|
| `render_console_report()` (사람용 콘솔) | `crates/marknest/src/lib.rs` | L1715 |
| `write_json_report()` (JSON report) | `crates/marknest/src/lib.rs` | L1258 |
| `ValidationReport` struct | `crates/marknest/src/lib.rs` | L2659 |

### P1-5. Exit code 정책

| Exit Code | 의미 | 파일 | 위치 |
|-----------|------|------|------|
| `0` (`EXIT_SUCCESS`) | 검증 성공 | `crates/marknest/src/lib.rs` | L20 |
| `1` (`EXIT_WARNING`) | 경고 포함 성공 | `crates/marknest/src/lib.rs` | L21 |
| `2` (`EXIT_VALIDATION_FAILURE`) | 검증 실패 | `crates/marknest/src/lib.rs` | L22 |
| `3` (`EXIT_SYSTEM_FAILURE`) | 시스템 실패 | `crates/marknest/src/lib.rs` | L23 |

Exit code 결정 로직: `build_validation_report()` (L1101-1114)

### P1-6. 대표 사용자 에러 메시지

| 메시지 | 파일 | 위치 |
|--------|------|------|
| "Entry markdown file could not be found: {path}" | `crates/marknest/src/lib.rs` | L932 |
| "Multiple markdown files were detected. Use --entry or --all." | `crates/marknest/src/lib.rs` | L980 |
| "No entry markdown file was found inside the ZIP input." | `crates/marknest/src/lib.rs` | L997 |
| "Missing asset: {path}" | `crates/marknest/src/lib.rs` | L1095 |
| "Invalid asset path: {error}" | `crates/marknest/src/lib.rs` | L1078 |

### P1-7. CLI 통합 테스트

| 테스트 | 파일 |
|--------|------|
| 단일 md validate | `crates/marknest/tests/validate.rs` |
| zip validate | `crates/marknest/tests/validate.rs` |
| folder validate (--all) | `crates/marknest/tests/validate.rs` |
| --strict 모드 | `crates/marknest/tests/validate.rs` |
| JSON report 생성 | `crates/marknest/tests/validate.rs` |
| 다중 md 에러 처리 | `crates/marknest/tests/validate.rs` |

---

## Phase 2: CLI Single Convert Basic

> 현재 경로 또는 지정 `.md` 단일 변환, 기본 이미지 렌더, 기본 page size/margin

### P2-1. 단일 변환 명령

| 구현 | 파일 | 위치 |
|------|------|------|
| `parse_convert_args()` | `crates/marknest/src/lib.rs` | L1988 |
| `ConvertCliArgs` (CLI 옵션 원본) | `crates/marknest/src/lib.rs` | L2235 |
| `ConvertArgs` (해석된 최종 인자) | `crates/marknest/src/lib.rs` | L2268 |
| `run_convert()` (변환 디스패처) | `crates/marknest/src/lib.rs` | L120 |
| `run_single_convert()` | `crates/marknest/src/lib.rs` | L335 |
| `convert_entry_to_pdf()` | `crates/marknest/src/lib.rs` | L491 |

### P2-2. raw HTML img 이미지 렌더링

| 구현 | 파일 | 위치 |
|------|------|------|
| `rewrite_html_img_sources()` (data URI 치환) | `crates/marknest-core/src/lib.rs` | - |
| `materialize_remote_assets_for_entry()` | `crates/marknest/src/lib.rs` | L1359 |
| `fetch_remote_asset_data_uri()` (HTTP fetch → data URI) | `crates/marknest/src/lib.rs` | L1378 |

원격 에셋 제한:
- 타임아웃: 15초 (L34)
- 최대 리다이렉트: 5회 (L35)
- 파일당 최대: 16 MB (L36)
- 전체 최대: 64 MB (L37)

### P2-3. PDF 생성 (Page Size / Margin)

| 구현 | 파일 | 위치 |
|------|------|------|
| `PdfPageSize` enum (A4, Letter) | `crates/marknest/src/lib.rs` | L2529 |
| `PdfMarginsMm` struct (top/right/bottom/left) | `crates/marknest/src/lib.rs` | L2534 |
| 기본 마진: 16.0 mm | `crates/marknest/src/lib.rs` | L2544 |
| `parse_optional_page_size()` | `crates/marknest/src/lib.rs` | L3266 |
| `resolve_pdf_margins()` | `crates/marknest/src/lib.rs` | L3102 |
| `PdfRenderer` trait | `crates/marknest/src/lib.rs` | L2523 |
| `NodeBrowserPdfRenderer` (Playwright 기반) | `crates/marknest/src/lib.rs` | L2739 |
| `PLAYWRIGHT_PRINT_SCRIPT` (내장 JS) | `crates/marknest/src/lib.rs` | L24 |
| `playwright_print.js` (Playwright 스크립트) | `crates/marknest/src/playwright_print.js` | - |

---

## Phase 3: CLI Single Convert Advanced

> Mermaid/Math auto, orientation, theme, metadata 등 단일 문서 품질 옵션

### P3-1. Mermaid auto 모드

| 구현 | 파일 | 위치 |
|------|------|------|
| `MermaidMode` enum (Off, Auto, On) | `crates/marknest-core/src/lib.rs` | - |
| `parse_mermaid_mode()` | `crates/marknest/src/lib.rs` | L3010 |
| CLI 플래그: `--mermaid <off\|auto\|on>` | `crates/marknest/src/lib.rs` | L1988+ |
| 설정 파일: `mermaid` 필드 | `crates/marknest/src/lib.rs` | L3082 |
| 기본값: `MermaidMode::Auto` | `crates/marknest/src/lib.rs` | L3158 |
| `--mermaid-timeout-ms` | `crates/marknest/src/lib.rs` | - |

### P3-2. Math auto 모드

| 구현 | 파일 | 위치 |
|------|------|------|
| `MathMode` enum (Off, Auto, On) | `crates/marknest-core/src/lib.rs` | - |
| `parse_math_mode()` | `crates/marknest/src/lib.rs` | L3024 |
| CLI 플래그: `--math <off\|auto\|on>` | `crates/marknest/src/lib.rs` | L1988+ |
| 설정 파일: `math` 필드 | `crates/marknest/src/lib.rs` | L3084 |
| 기본값: `MathMode::Auto` | `crates/marknest/src/lib.rs` | L3162 |
| `--math-timeout-ms` | `crates/marknest/src/lib.rs` | - |

### P3-3. Orientation (가로/세로)

| 구현 | 파일 | 위치 |
|------|------|------|
| CLI 플래그: `--landscape` | `crates/marknest/src/lib.rs` | L2255 |
| 설정 파일: `landscape` 필드 | `crates/marknest/src/lib.rs` | L3139 |
| Playwright 전달 | `crates/marknest/src/lib.rs` | L2776 |

### P3-4. Theme Preset

| 구현 | 파일 | 위치 |
|------|------|------|
| `ThemePreset` enum (Default, Github, Docs, Plain) | `crates/marknest-core/src/lib.rs` | L91 |
| `theme_stylesheet()` (프리셋별 CSS) | `crates/marknest-core/src/lib.rs` | L1614-1627 |
| `base_stylesheet()` (공통 CSS) | `crates/marknest-core/src/lib.rs` | L1610 |
| `parse_optional_theme()` / `parse_theme_preset()` | `crates/marknest/src/lib.rs` | L3254 |
| CLI 플래그: `--theme <default\|github\|docs\|plain>` | `crates/marknest/src/lib.rs` | - |
| 환경변수: `MARKNEST_THEME` | `crates/marknest/src/lib.rs` | L3080 |

### P3-5. PDF 메타데이터

| 구현 | 파일 | 위치 |
|------|------|------|
| `PdfMetadata` struct (title, author, subject) | `crates/marknest/src/lib.rs` | L2150 |
| CLI 플래그: `--title`, `--author`, `--subject` | `crates/marknest/src/lib.rs` | - |
| `apply_pdf_metadata()` | `crates/marknest/src/lib.rs` | L578 |

### P3-6. Custom CSS

| 구현 | 파일 | 위치 |
|------|------|------|
| CLI 플래그: `--css <PATH>` | `crates/marknest/src/lib.rs` | - |
| 환경변수: `MARKNEST_CSS` | `crates/marknest/src/lib.rs` | L3119 |
| `load_render_support_files()` | `crates/marknest/src/lib.rs` | L604 |

### P3-7. Header/Footer 템플릿

| 구현 | 파일 | 위치 |
|------|------|------|
| CLI 플래그: `--header-template`, `--footer-template` | `crates/marknest/src/lib.rs` | - |
| `prepare_print_template()` | `crates/marknest/src/lib.rs` | L558-567 |
| `prepare_print_template_html()` | `crates/marknest/src/lib.rs` | L661-687 |
| 템플릿 변수: `{{title}}`, `{{pageNumber}}`, `{{totalPages}}` | `crates/marknest/src/lib.rs` | L661+ |
| XSS 방지: `<script`, `javascript:` 검증 | `crates/marknest/src/lib.rs` | L697+ |

### P3-8. TOC (목차)

| 구현 | 파일 | 위치 |
|------|------|------|
| CLI 플래그: `--toc`, `--no-toc` | `crates/marknest/src/lib.rs` | L2256 |
| 환경변수: `MARKNEST_TOC` | `crates/marknest/src/lib.rs` | - |
| `RenderedHeading` struct | `crates/marknest-core/src/lib.rs` | L242-262 |
| TOC CSS (`.marknest-toc`) | `crates/marknest-core/src/lib.rs` | L1610 |

### P3-9. HTML Sanitization

| 구현 | 파일 | 위치 |
|------|------|------|
| CLI 플래그: `--sanitize-html`, `--no-sanitize-html` | `crates/marknest/src/lib.rs` | L2257 |
| 환경변수: `MARKNEST_SANITIZE_HTML` | `crates/marknest/src/lib.rs` | - |
| `sanitize_html_fragment()` (ammonia 라이브러리) | `crates/marknest-core/src/lib.rs` | L1599-1607 |
| 허용 태그: details, figure, input, nav, summary 등 | `crates/marknest-core/src/lib.rs` | L1599+ |
| 기본값: enabled | `crates/marknest/src/lib.rs` | L3149 |

### P3-10. 렌더 실패 warning/error 정책

| 구현 | 파일 | 위치 |
|------|------|------|
| `map_render_error()` | `crates/marknest/src/lib.rs` | L889 |
| `PdfRenderOutcome` (warnings 포함) | `crates/marknest/src/lib.rs` | L2497 |
| 콘솔 출력: "Conversion succeeded/completed with warnings" | `crates/marknest/src/lib.rs` | L1799 |

---

## Phase 4: CLI Batch and Workspace

> ZIP 단일 엔트리 변환, ZIP/폴더 `--all` 일괄 변환, 경로 보존, 충돌 처리

### P4-1. ZIP 단일 엔트리 변환

| 구현 | 파일 | 위치 |
|------|------|------|
| `materialize_zip_workspace()` (ZIP 추출) | `crates/marknest/src/lib.rs` | L281 |
| `prepare_render_workspace()` (임시 디렉터리 준비) | `crates/marknest/src/lib.rs` | L257 |
| `--entry <PATH>` 옵션 | `crates/marknest/src/lib.rs` | L1996-2001 |
| 경로 안전 검증 (`normalize_relative_string`) | `crates/marknest/src/lib.rs` | L306 |

### P4-2. ZIP/폴더 `--all` 일괄 변환

| 구현 | 파일 | 위치 |
|------|------|------|
| `--all` 플래그 파싱 | `crates/marknest/src/lib.rs` | L2003 |
| `convert_mode_from_selection()` | `crates/marknest/src/lib.rs` | L196 |
| `run_batch_convert()` | `crates/marknest/src/lib.rs` | L391 |
| `--out-dir <PATH>` 옵션 | `crates/marknest/src/lib.rs` | L2013-2018 |
| 폴더 입력 시 암묵적 `--all` | `crates/marknest/src/lib.rs` | L946-966 |

### P4-3. 상대 경로 보존 출력

| 구현 | 파일 | 위치 |
|------|------|------|
| `batch_output_path()` | `crates/marknest/src/lib.rs` | L1227 |
| `plan_batch_output_targets()` | `crates/marknest/src/lib.rs` | L1179 |

예: `docs/guide.md` → `out_dir/docs/guide.pdf`

### P4-4. 파일명 충돌 처리

| 구현 | 파일 | 위치 |
|------|------|------|
| `output_collision_key()` (대소문자 정규화) | `crates/marknest/src/lib.rs` | L1236-1242 |
| `ConvertCollisionReport` struct | `crates/marknest/src/lib.rs` | - |
| 충돌 콘솔 출력 | `crates/marknest/src/lib.rs` | L1769-1777 |

### P4-5. Batch Summary

| 구현 | 파일 | 위치 |
|------|------|------|
| `ConvertReport` struct | `crates/marknest/src/lib.rs` | L2682 |
| `build_convert_report()` | `crates/marknest/src/lib.rs` | L403 |
| `finalize_convert_report()` | `crates/marknest/src/lib.rs` | L471 |
| `render_batch_convert_console_output()` | `crates/marknest/src/lib.rs` | L1752 |

Report 포함 정보:
- `status` (Success/Warning/Failure)
- `outputs` (성공한 변환 목록)
- `failures` (실패한 변환 목록)
- `collisions` (경로 충돌 목록)
- `warnings`, `errors`

### P4-6. 설정 파일 (TOML)

| 구현 | 파일 | 위치 |
|------|------|------|
| `MarknestConfigFile` (최상위 TOML 구조) | `crates/marknest/src/lib.rs` | L2294 |
| `ConvertConfigFile` (convert 섹션) | `crates/marknest/src/lib.rs` | L2299 |
| `resolve_convert_config_path()` | `crates/marknest/src/lib.rs` | L3176 |
| `load_convert_config_file()` | `crates/marknest/src/lib.rs` | L3195 |

옵션 우선순위: CLI 인자 > 설정 파일 > 환경변수 > 기본값 (L3104-3173)

### P4-7. 디버그 산출물

| 구현 | 파일 | 위치 |
|------|------|------|
| `--debug-html <PATH>` | `crates/marknest/src/lib.rs` | L531 |
| `--asset-manifest <PATH>` | `crates/marknest/src/lib.rs` | L535 |
| `AssetManifest` struct | `crates/marknest/src/lib.rs` | L2439 |
| `--render-report <PATH>` (JSON 리포트) | `crates/marknest/src/lib.rs` | L375, L473 |

---

## Phase 5: Web Analyze and Preview

> ZIP 업로드, WASM 기반 분석, 엔트리 선택 UI, 누락 에셋 경고, HTML preview

### P5-1. ZIP 업로드

| 구현 | 파일 | 위치 |
|------|------|------|
| 업로드 패널 UI (dropzone) | `index.html` | L44-54 |
| `zip-input` 파일 입력 | `index.html` | L52 |
| 파일 변경 이벤트 핸들러 | `web/app.js` | L1044-1051 |

### P5-2. WASM 기반 분석 (analyzeZip)

| 구현 | 파일 | 위치 |
|------|------|------|
| `analyze_zip_binding()` (WASM 바인딩) | `crates/marknest-wasm/src/lib.rs` | L223-229 |
| JS 이름: `analyzeZip` | `crates/marknest-wasm/src/lib.rs` | L223 |
| `analyze_zip_model()` (내부 모델 변환) | `crates/marknest-wasm/src/lib.rs` | L281-283 |
| Core 호출: `marknest_core::analyze_zip()` | `crates/marknest-core/src/lib.rs` | L268-270 |
| Web 핸들러: `analyzeZip()` | `web/app.js` | L459-521 |
| 상태 관리: `state.projectIndex`, `state.zipBytes` | `web/app.js` | L459+ |

### P5-3. 엔트리 선택 UI

| 구현 | 파일 | 위치 |
|------|------|------|
| 엔트리 목록 패널 | `index.html` | L261-269 |
| `renderEntries()` (목록 렌더링) | `web/app.js` | L347-380 |
| 자동 선택 로직 | `web/app.js` | L481-482 |
| 클릭 → `renderPreview()` | `web/app.js` | L364 |

### P5-4. 누락 에셋 경고

| 구현 | 파일 | 위치 |
|------|------|------|
| `buildBrowserAssetManifest()` | `web/app.js` | L264-280 |
| `syncDiagnosticLists()` | `web/app.js` | L167-176 |
| 경고 UI: `warning-list`, `error-list` | `index.html` | L279-287 |

### P5-5. HTML Preview 렌더링 (WASM 기반)

| 구현 | 파일 | 위치 |
|------|------|------|
| `render_html_binding()` (WASM 바인딩) | `crates/marknest-wasm/src/lib.rs` | L231-243 |
| JS 이름: `renderHtml` | `crates/marknest-wasm/src/lib.rs` | L231 |
| `render_preview_model()` (내부 모델 변환) | `crates/marknest-wasm/src/lib.rs` | L295-308 |
| Core 호출: `render_zip_entry_with_options()` | `crates/marknest-core/src/lib.rs` | L232-239 |
| `RenderPreview` struct (title, html) | `crates/marknest-wasm/src/lib.rs` | L26-29 |
| Web 핸들러: `renderPreview()` | `web/app.js` | L571-643 |
| iframe 로딩: `loadFrameSrcdoc()` | `web/app.js` | L599 |

### P5-6. 출력 옵션 컨트롤 UI

| 구현 | 파일 | 위치 |
|------|------|------|
| Theme 선택기 | `index.html` | L131-139 |
| Page size, 마진 | `index.html` | L140-162 |
| Landscape, TOC, Sanitize 토글 | `index.html` | L163-183 |
| Mermaid, Math 모드 | `index.html` | L184-199 |
| Custom CSS textarea | `index.html` | L201-209 |
| 메타데이터 (title, author, subject) | `index.html` | L211-223 |
| Header/Footer 템플릿 | `index.html` | L224-253 |
| `buildOutputOptions()` (옵션 수집) | `web/output_options.mjs` | L51-86 |
| `BrowserOutputOptions` (WASM 구조체) | `crates/marknest-wasm/src/lib.rs` | L54-74 |

### P5-7. 원격 이미지 브라우저 Materialization

| 구현 | 파일 | 위치 |
|------|------|------|
| `materializeRemoteImages()` | `web/remote_assets.mjs` | L263-319 |
| 에셋당 최대: 16 MB | `web/remote_assets.mjs` | L1 |
| 전체 최대: 64 MB | `web/remote_assets.mjs` | L3 |
| 타임아웃: 15초 | `web/remote_assets.mjs` | L267 |

---

## Phase 6: Web Browser PDF

> 브라우저 기반 단일 PDF 생성, 일괄 변환 ZIP 다운로드

### P6-1. 브라우저 기반 단일 PDF 생성

| 구현 | 파일 | 위치 |
|------|------|------|
| `buildPdfBlobFromPreview()` | `web/app.js` | L662-719 |
| `window.html2pdf()` 호출 | `web/app.js` | L703-713 |
| html2pdf.js 번들 에셋 | `runtime-assets/html2pdf/html2pdf.bundle.min.js` | - |
| `downloadSelectedPdf()` | `web/app.js` | L944-973 |
| 다운로드 버튼 | `index.html` | L108 |

### P6-2. 일괄 변환 ZIP 다운로드

| 구현 | 파일 | 위치 |
|------|------|------|
| `downloadBatchZipInBrowser()` | `web/app.js` | L853-897 |
| `renderHtmlBatch` (WASM 배치 렌더) | `crates/marknest-wasm/src/lib.rs` | L245-260 |
| `render_preview_batch_model()` | `crates/marknest-wasm/src/lib.rs` | L310-330 |
| `build_pdf_archive_binding()` (ZIP 패키징) | `crates/marknest-wasm/src/lib.rs` | L262-267 |
| `build_pdf_archive_model()` | `crates/marknest-wasm/src/lib.rs` | L430-449 |
| 다운로드 버튼 | `index.html` | L111 |

### P6-3. 디버그 번들 다운로드

| 구현 | 파일 | 위치 |
|------|------|------|
| `downloadDebugBundle()` | `web/app.js` | L906-942 |
| `build_debug_bundle_model()` | `crates/marknest-wasm/src/lib.rs` | L332-375 |
| `BrowserAssetManifest` | `crates/marknest-wasm/src/lib.rs` | L77-83 |
| `BrowserRenderReport` | `crates/marknest-wasm/src/lib.rs` | L86-95 |
| `BrowserRuntimeInfo` | `crates/marknest-wasm/src/lib.rs` | L98-109 |
| 다운로드 버튼 | `index.html` | L114 |

번들 내용: `debug.html`, `asset-manifest.json`, `render-report.json`, 런타임 에셋

### P6-4. 진행 상태 표시

| 구현 | 파일 | 위치 |
|------|------|------|
| `setStatus()` | `web/app.js` | L95-99 |
| 상태 UI: `status-chip`, `status-message` | `web/app.js` | L40-80 |
| 상태 종류: "waiting", "ready", "warning", "error" | `web/app.js` | L95+ |
| 배치 진행률: "Exporting (N of M)" | `web/app.js` | L860-892 |

### P6-5. 내보내기 백엔드 결정 (브라우저 vs 서버)

| 구현 | 파일 | 위치 |
|------|------|------|
| `resolveExportBackend()` | `web/export_policy.mjs` | L46-60 |
| "high-quality" → 서버 | `web/export_policy.mjs` | L51-52 |
| 브라우저 실패 + fallback → 서버 | `web/export_policy.mjs` | L55-56 |
| 기본 → 브라우저 | `web/export_policy.mjs` | L59 |
| `downloadSelectedPdfFromServer()` | `web/app.js` | L842-851 |
| `downloadBatchZipFromServer()` | `web/app.js` | L899-904 |
| `buildFallbackFormData()` | `web/output_options.mjs` | L88-101 |

### P6-6. Runtime 동기화

| 구현 | 파일 | 위치 |
|------|------|------|
| `runtime_sync.mjs` (Mermaid/Math 완료 대기) | `web/runtime_sync.mjs` | - |
| `window.__MARKNEST_RENDER_STATUS__` 감시 | `web/runtime_sync.mjs` | - |

---

## Phase 7: Server Fallback and Scale

> Playwright/Chromium 기반 fallback, 대용량 ZIP 가드, 브라우저 실패 시 서버 전환

### P7-1. Playwright/Chromium 기반 Fallback 서버

| 구현 | 파일 | 위치 |
|------|------|------|
| `PdfFallbackExporter` trait | `crates/marknest-server/src/lib.rs` | L71 |
| `ChromiumFallbackExporter` struct | `crates/marknest-server/src/lib.rs` | L84 |
| `export_selected_pdf()` | `crates/marknest-server/src/lib.rs` | L71 |
| `export_batch_archive()` | `crates/marknest-server/src/lib.rs` | L71 |
| 서버 main 진입점 | `crates/marknest-server/src/main.rs` | - |

### P7-2. API 엔드포인트

| 엔드포인트 | 메서드 | 파일 | 기능 |
|-----------|--------|------|------|
| `/api/render/pdf` | POST | `crates/marknest-server/src/lib.rs` | 단일 PDF 렌더 |
| `/api/render/batch` | POST | `crates/marknest-server/src/lib.rs` | 배치 PDF → ZIP |
| `/api/health` | GET | `crates/marknest-server/src/lib.rs` | 헬스체크 |

- 프레임워크: Axum
- CORS, tracing 미들웨어
- Multipart 64 MB 제한
- 필드: `entry`, `options` (JSON), `archive` (ZIP bytes)

### P7-3. Fallback 렌더 옵션

| 구현 | 파일 | 위치 |
|------|------|------|
| `FallbackRenderOptions` struct | `crates/marknest-server/src/lib.rs` | L93 |

지원 옵션: theme, custom_css, enable_toc, sanitize_html, title/author/subject, page_size, margins (전체/개별), landscape, header/footer 템플릿, mermaid/math 모드 및 타임아웃

### P7-4. 대용량 ZIP 가드

| 구현 | 파일 | 위치 |
|------|------|------|
| `MAX_ZIP_FILE_COUNT = 4_096` | `crates/marknest-core/src/lib.rs` | L12 |
| `MAX_ZIP_UNCOMPRESSED_BYTES = 256 MB` | `crates/marknest-core/src/lib.rs` | L13 |
| `AnalyzeError::ZipLimitsExceeded` | `crates/marknest-core/src/lib.rs` | L153+ |
| 웹 대용량 안내 | `web/app.js` | - |

### P7-5. 브라우저 실패 → 서버 전환

| 구현 | 파일 | 위치 |
|------|------|------|
| `resolveExportBackend()` | `web/export_policy.mjs` | L46-60 |
| 브라우저 실패 + fallback URL → 자동 서버 전환 | `web/export_policy.mjs` | L55-56 |
| 원격 이미지 실패 시 서버 재시도 | `web/app.js` | L960-967 |
| `state.fallbackBaseUrl` (서버 URL 저장) | `web/app.js` | L35 |
| 품질 모드 선택 UI | `index.html` | - |

### P7-6. 브라우저 경로 탐색

| 구현 | 파일 | 위치 |
|------|------|------|
| `MARKNEST_BROWSER_PATH` 환경변수 우선 | `crates/marknest/src/lib.rs` | L2920 |
| Chrome/Edge/Chromium/Brave 경로 탐색 | `crates/marknest/src/lib.rs` | L2920+ |
| macOS, Linux, Windows 플랫폼별 경로 | `crates/marknest/src/lib.rs` | L2920+ |

### P7-7. Reusable HTML-to-PDF Helper

| 구현 | 파일 | 위치 |
|------|------|------|
| `HtmlToPdfRequest` struct | `crates/marknest/src/lib.rs` | - |
| `HtmlToPdfResult` struct | `crates/marknest/src/lib.rs` | - |
| `html_to_pdf()` (재사용 가능 함수) | `crates/marknest/src/lib.rs` | - |

---

## Phase 8: Output Quality and Operations

> 스타일, 설정, Docker, runtime pinning, 재현성, 운영 관측성

### P8-1. 스타일 프리셋

| 구현 | 파일 | 위치 |
|------|------|------|
| `ThemePreset` enum | `crates/marknest-core/src/lib.rs` | L91 |
| `theme_stylesheet()` | `crates/marknest-core/src/lib.rs` | L1614-1627 |
| `base_stylesheet()` (공통 기본 스타일) | `crates/marknest-core/src/lib.rs` | L1610 |
| `runtime_stylesheet()` (Math/Mermaid SVG) | `crates/marknest-core/src/lib.rs` | - |

프리셋:
- `.theme-default`: 기본
- `.theme-github`: Segoe UI, GitHub 색상, 밝은 배경
- `.theme-docs`: Georgia serif, slate blue 헤딩
- `.theme-plain`: 최소 스타일, 투명 코드블록

### P8-2. 사용자 CSS 오버라이드

| 구현 | 파일 | 위치 |
|------|------|------|
| `--css <PATH>` | `crates/marknest/src/lib.rs` | - |
| `MARKNEST_CSS` 환경변수 | `crates/marknest/src/lib.rs` | L3119 |
| 설정 파일: `css` 필드 | `crates/marknest/src/lib.rs` | - |
| 적용 순서: base → theme → user CSS | `crates/marknest-core/src/lib.rs` | L1586 |

### P8-3. Header/Footer 및 페이지 번호

| 구현 | 파일 | 위치 |
|------|------|------|
| `prepare_print_template_html()` | `crates/marknest/src/lib.rs` | L661-687 |
| 지원 토큰: `{{title}}`, `{{pageNumber}}`, `{{totalPages}}` | `crates/marknest/src/lib.rs` | L661+ |
| 보안 검증 (script, javascript: 차단) | `crates/marknest/src/lib.rs` | L697+ |
| HTML 이스케이핑 | `crates/marknest/src/lib.rs` | L697-704 |

### P8-4. 설정 파일 / 환경변수

| 구현 | 파일 | 위치 |
|------|------|------|
| `MarknestConfigFile` (TOML 구조) | `crates/marknest/src/lib.rs` | L2294 |
| `ConvertConfigFile` (convert 섹션) | `crates/marknest/src/lib.rs` | L2299 |
| `resolve_convert_config_path()` | `crates/marknest/src/lib.rs` | L3176 |
| `load_convert_config_file()` | `crates/marknest/src/lib.rs` | L3195 |

환경변수 목록:

| 환경변수 | 용도 |
|----------|------|
| `MARKNEST_CONFIG` | 설정 파일 경로 |
| `MARKNEST_THEME` | 테마 프리셋 |
| `MARKNEST_CSS` | CSS 파일 경로 |
| `MARKNEST_TOC` | 목차 활성화 (true/false) |
| `MARKNEST_SANITIZE_HTML` | HTML 새니타이즈 (true/false) |
| `MARKNEST_MERMAID_TIMEOUT_MS` | Mermaid 타임아웃 |
| `MARKNEST_MATH_TIMEOUT_MS` | Math 타임아웃 |
| `MARKNEST_NODE_PATH` | Node.js 실행 경로 |
| `MARKNEST_BROWSER_PATH` | Chromium/Chrome 경로 |
| `MARKNEST_PLAYWRIGHT_RUNTIME_DIR` | Playwright 패키지 경로 |
| `MARKNEST_SERVER_ADDR` | 서버 바인딩 주소 |

### P8-5. 공식 Docker 이미지

| 구현 | 파일 | 위치 |
|------|------|------|
| Dockerfile | `Dockerfile` | - |

빌드 단계:
1. `builder`: Rust 컴파일 (rust:1.86.0-slim-bookworm)
2. `playwright-runtime`: Node.js 의존성 (node:22-bookworm-slim)
3. `final`: Debian bookworm-slim 런타임

포함 패키지:
- `chromium`, `nodejs`
- `fonts-dejavu-core`, `fonts-noto-cjk` (CJK 폰트)
- CA certificates

환경 설정:
- `MARKNEST_BROWSER_PATH=/usr/bin/chromium`
- `MARKNEST_PLAYWRIGHT_RUNTIME_DIR=/opt/marknest/playwright-runtime`
- `MARKNEST_SERVER_ADDR=0.0.0.0:3476`
- 포트: 3476
- 엔트리포인트: `marknest-server`

### P8-6. Runtime Pinning

| 구현 | 파일 | 위치 |
|------|------|------|
| `MERMAID_VERSION = "11.11.0"` | `crates/marknest-core/src/lib.rs` | L17 |
| `MATHJAX_VERSION = "3.2.2"` | `crates/marknest-core/src/lib.rs` | L18 |
| `RUNTIME_ASSET_MODE = "bundled_local"` | `crates/marknest-core/src/lib.rs` | L20 |
| Playwright 버전: 1.58.2 | `crates/marknest/src/lib.rs` | L25 |

번들 에셋 (`runtime-assets/`):
- `mermaid/mermaid.min.js`
- `mathjax/es5/tex-svg.js`
- `html2pdf/html2pdf.bundle.min.js`
- `licenses/` (서드파티 라이선스)

### P8-7. 디버그 번들 및 재현성

| 구현 | 파일 | 위치 |
|------|------|------|
| `--debug-html` (렌더된 HTML 저장) | `crates/marknest/src/lib.rs` | L531 |
| `write_debug_runtime_assets()` | `crates/marknest/src/lib.rs` | L1349 |
| `--asset-manifest` (에셋 인벤토리) | `crates/marknest/src/lib.rs` | L535 |
| `AssetManifest` struct | `crates/marknest/src/lib.rs` | L2439 |
| `--render-report` (변환 리포트) | `crates/marknest/src/lib.rs` | L375, L473 |

### P8-8. 운영 관측성

| 구현 | 파일 | 위치 |
|------|------|------|
| `tracing` + `tracing_subscriber` (로깅) | `crates/marknest/src/lib.rs` | - |
| `ConvertReport` (변환 리포트) | `crates/marknest/src/lib.rs` | L2682-2700 |
| `ValidationReport` (검증 리포트) | `crates/marknest/src/lib.rs` | L2659 |
| 런타임 정보 (renderer, node, playwright, browser) | `crates/marknest/src/lib.rs` | L2724-2729 |
| 서버 요청 로그 (structured tracing) | `crates/marknest-server/src/lib.rs` | - |

### P8-9. 인쇄 레이아웃 가드

| 구현 | 파일 | 위치 |
|------|------|------|
| `h1 { break-before: page }` (섹션 시작 페이지 나눔) | `crates/marknest-core/src/lib.rs` | L1610 |
| `h1:first-of-type { break-before: auto }` | `crates/marknest-core/src/lib.rs` | L1610 |
| `pre, table, blockquote ... { break-inside: avoid }` | `crates/marknest-core/src/lib.rs` | L1610 |
| `thead { display: table-header-group }` | `crates/marknest-core/src/lib.rs` | L1610 |

### P8-10. Emoji 숏코드

| 구현 | 파일 | 위치 |
|------|------|------|
| `replace_github_emoji_shortcodes()` | `crates/marknest-core/src/lib.rs` | L1143-1168 |
| `emojis` 크레이트 (`get_by_shortcode()`) | `crates/marknest-core/src/lib.rs` | - |
| 코드 블록 제외, prose 텍스트만 적용 | `crates/marknest-core/src/lib.rs` | L1143+ |

### P8-11. Heading Anchor 생성

| 구현 | 파일 | 위치 |
|------|------|------|
| Heading `id` 속성 생성 (마크다운 파싱 중) | `crates/marknest-core/src/lib.rs` | - |
| TOC에서 `#id` 프래그먼트 링크 사용 | `crates/marknest-core/src/lib.rs` | - |

### P8-12. PDF Fidelity 검증 인프라

| 구현 | 파일 | 위치 |
|------|------|------|
| 60-entry README corpus manifest | `validation/readme-corpus-60.tsv` | - |
| `readme_corpus.mjs` (검증 러너) | `validation/readme_corpus.mjs` | - |
| `baseline_artifacts.mjs` | `validation/lib/baseline_artifacts.mjs` | - |
| `diff_policy.mjs` | `validation/lib/diff_policy.mjs` | - |
| `manifest.mjs` | `validation/lib/manifest.mjs` | - |
| `png_metrics.mjs` (이미지 비교) | `validation/lib/png_metrics.mjs` | - |
| `text_metrics.mjs` (텍스트 커버리지) | `validation/lib/text_metrics.mjs` | - |
| README corpus baseline artifacts root | `validation/baselines/readme-corpus-60/` | - |

Blocking 기준:
- 변환 exit code > 1
- render report status = failure
- 로컬 에셋 누락
- H1/H2/H3 헤딩 PDF 텍스트에서 누락
- 토큰 커버리지 < 0.97
- 빈 페이지 감지

---

## 전체 요약

| Phase | 범위 | 상태 | 주요 코드 위치 |
|-------|------|------|--------------|
| **Phase 0** | Core Analyze Foundation | 완료 | `crates/marknest-core/src/lib.rs` |
| **Phase 1** | CLI Validate | 완료 | `crates/marknest/src/lib.rs`, `tests/validate.rs` |
| **Phase 2** | CLI Single Convert Basic | 완료 | `crates/marknest/src/lib.rs`, `playwright_print.js` |
| **Phase 3** | CLI Single Convert Advanced | 완료 | `crates/marknest/src/lib.rs`, `crates/marknest-core/src/lib.rs` |
| **Phase 4** | CLI Batch and Workspace | 완료 | `crates/marknest/src/lib.rs` |
| **Phase 5** | Web Analyze and Preview | 완료 | `crates/marknest-wasm/src/lib.rs`, `web/app.js`, `index.html` |
| **Phase 6** | Web Browser PDF | 완료 | `web/app.js`, `web/export_policy.mjs`, `web/output_options.mjs` |
| **Phase 7** | Server Fallback and Scale | 완료 | `crates/marknest-server/src/lib.rs`, `web/export_policy.mjs` |
| **Phase 8** | Output Quality and Operations | 완료 | `Dockerfile`, `runtime-assets/`, `validation/` |

모든 Phase(0~8)의 요구사항이 구현 완료되었음.
