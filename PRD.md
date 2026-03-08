# PRD: MarkNest

> MarkNest: Zip/워크스페이스 기반 Markdown(+이미지) to PDF 제품군

## 1. 문서 정보
- 문서명: `PRD.md`
- 버전: `v1.8`
- 작성일: `2026-03-08`
- 제품명: `MarkNest`
- CLI 바이너리명: `marknest`
- 기본 설정 파일: `.marknest.toml`, `marknest.toml`
- 대상 제품:
  - Rust 코어 라이브러리 (WASM 지원)
  - CLI
  - 웹 애플리케이션

## 2. 배경 및 문제 정의
일반적인 웹 Markdown to PDF 변환기는 단일 `.md` 파일 중심이며, 상대경로로 연결된 로컬 이미지(`./images/a.png`)를 안정적으로 처리하지 못하는 경우가 많다. 사용자는 문서 폴더 전체를 올리지 못하거나, 이미지 링크가 깨진 상태로 PDF를 받는다.

우리가 해결하려는 핵심 문제는 다음과 같다.
- 문서와 이미지가 함께 있는 프로젝트 구조를 손쉽게 업로드하고 싶다.
- 상대경로 링크를 보존한 채 정확히 렌더링하고 싶다.
- 브라우저 내 처리(Rust+WASM)로 빠르게 결과를 확인하고 싶다.
- 필요 시 서버 fallback으로 고품질 PDF를 안정적으로 생성하고 싶다.
- 로컬 개발 환경에서는 현재 경로의 md 파일을 바로 변환하고 싶다.
- ZIP 또는 폴더 내 md가 여러 개인 경우에도 단일/일괄 변환을 제어하고 싶다.
- Mermaid 코드블록도 깨지지 않게 PDF에 반영하고 싶다.
- 수식(Math)과 raw HTML `<img>`도 PDF 렌더 결과에 반영하고 싶다.

## 3. 제품 비전
"`zip 하나 업로드하면, md와 연결된 이미지까지 정확히 포함된 PDF를 안전하고 빠르게 생성하는 표준 워크플로`"를 제공한다.

## 4. 목표 / 비목표

### 4.1 목표 (Goals)
- 웹에서 `.zip` 업로드만으로 Markdown+이미지 PDF 생성 성공률을 높인다.
- 웹 기본 경로는 사용자가 업로드한 ZIP/Markdown 내용을 MarkNest backend로 전송하지 않는다.
- Rust 코어를 단일 소스로 두고 `Library / WASM / CLI / Web`에서 재사용한다.
- 경로 해석 및 보안(Zip Slip, 확장자 제한, 용량 제한)을 기본 제공한다.
- MVP에서 실사용 가능한 품질(텍스트/이미지/표/코드블록)을 제공한다.
- CLI에서 현재 경로의 md 변환 시 이미지 링크를 자동 해석한다.
- CLI에서도 ZIP 입력 변환/검증을 웹과 동일하게 지원한다.
- ZIP/폴더의 다중 md를 단일 또는 일괄(batch)로 변환할 수 있다.
- Mermaid 코드블록을 옵션 기반으로 SVG 렌더링해 PDF에 포함한다.
- 수식(Math)과 raw HTML `<img>`를 검증/렌더링 파이프라인에 포함한다.
- 수식 엔진은 MathJax로 표준화해 환경 간 일관성을 확보한다.
- 스타일 프리셋, 사용자 CSS, 헤더/푸터 템플릿으로 배포용 PDF 품질을 제어할 수 있다.
- 설정 파일, 환경변수, 디버그 산출물로 팀 단위 재현성과 운영 효율을 확보한다.
- 공식 Docker/runtime pinning을 통해 CLI와 서버 fallback의 실행 환경 일관성을 확보한다.

### 4.2 비목표 (Non-Goals, MVP)
- 출판급 정교한 paged media(복잡한 각주/인덱스 자동화) 완전 지원
- DOCX/PPTX 등 다문서 포맷 동시 변환
- 실시간 협업 편집 기능

## 5. 대상 사용자
- 기술 문서 작성자 (README/가이드 문서)
- 개발팀/DevRel (배포용 PDF 생성)
- 교육/사내 매뉴얼 작성자 (이미지 다수 포함 문서)

## 6. 핵심 사용자 시나리오
### 6.1 웹 시나리오 (ZIP 업로드)
1. 사용자가 `docs.zip` 업로드
2. 시스템이 브라우저에서 ZIP 해제 후 md 후보 파일 탐색
3. 시스템이 md 후보 목록을 표시하고 사용자가 변환 모드 선택
   - 단일 변환: 특정 md 선택
   - 일괄 변환: 전체 md 선택
4. 상대경로 이미지와 raw HTML `<img src>` 링크 유효성 검사
5. Mermaid/Math 블록 감지 및 렌더 옵션 적용
6. HTML 미리보기 렌더
7. PDF 생성
   - 기본: 브라우저 변환 (`Browser Fast`, backend 업로드 없음)
   - fallback: 사용자가 선택하거나 브라우저 경로 실패 시에만 로컬/서버 Chromium `printToPDF`
8. 결과 다운로드
   - 단일 변환: PDF 1개
   - 일괄 변환: PDF 묶음 ZIP 1개

### 6.5 오프라인/프라이버시 의미 정의
- 이 PRD에서 기본 경로의 `offline` 또는 `local-first` 의미는 "`업로드한 Markdown/ZIP 본문이 MarkNest backend로 기본 전송되지 않는다`"를 우선 의미로 한다.
- 이는 "`문서가 참조하는 외부 HTTP(S) 이미지/에셋으로의 직접 네트워크 요청까지 모두 금지한다`"와는 동일한 의미가 아니다.
- 즉, 브라우저 또는 네이티브 런타임이 문서 품질을 위해 외부 이미지 URL을 직접 fetch하는 것은 허용 범위에 포함될 수 있다.
- 완전한 air-gapped/no-network 모드는 별도 정책 또는 옵션으로 다루며, MVP의 기본 의미로 간주하지 않는다.

### 6.2 CLI 시나리오 (현재 경로 md 변환)
1. 사용자가 문서 폴더에서 `marknest convert` 또는 `marknest convert README.md` 실행
2. CLI가 엔트리 md를 자동 탐지 또는 지정 파일 사용
3. md 기준 상대경로 이미지 및 raw HTML `<img>` 링크를 자동 해석
4. Mermaid/Math 블록을 옵션(`--mermaid`, `--math`)에 따라 렌더 또는 스킵
5. 누락 이미지/다이어그램/수식 오류를 경고/에러로 출력
6. PDF 파일 생성

### 6.3 CLI 시나리오 (ZIP 변환)
1. 사용자가 `marknest convert docs.zip -o out.pdf` 실행
2. CLI가 ZIP을 안전하게 해제/인덱싱
3. 엔트리 md 자동 탐지 또는 `--entry`/`--all` 지정값 사용
4. 상대경로 이미지/raw HTML `<img>` 링크와 Mermaid/Math 렌더 가능 여부를 검증하고 리포트
5. PDF 파일 생성

### 6.4 CLI 시나리오 (폴더 일괄 변환)
1. 사용자가 `marknest convert ./docs --out-dir ./pdf` 실행
2. CLI가 하위 폴더를 재귀 탐색해 모든 `.md` 파일을 수집
3. 각 md의 상대경로 이미지/raw HTML `<img>`를 해석/검증하고 Mermaid/Math를 렌더
4. 입력 폴더 구조를 보존해 PDF를 일괄 생성
5. 변환 요약(성공/실패/누락 에셋)을 출력

## 7. 기능 요구사항

### 7.1 공통 코어(Rust Library)
- Markdown 파싱 및 HTML 변환
- 파일 시스템 추상화 (메모리 FS/실제 FS)
- 경로 정규화 및 안전 검사
- 엔트리 인덱서:
  - md 후보 수집
  - 단일/일괄 변환 대상 선택
  - 출력 경로 충돌 감지
- 에셋 해석기:
  - 상대경로 이미지 resolve
  - raw HTML `<img src>` resolve
  - 누락 파일 목록화
  - MIME 타입 추론
- Mermaid 처리기:
  - fenced code block(````mermaid`) 감지
  - Mermaid AST/텍스트 전달 인터페이스 제공
  - SVG 렌더 결과 주입 및 실패 fallback 처리
- Math 처리기:
  - inline/block 수식 감지
  - MathJax 기반 정적 렌더(HTML/SVG) 지원
  - 렌더 실패 fallback 처리
- 출력 옵션 모델:
  - 페이지 크기(A4/Letter)
  - 방향(Portrait/Landscape)
  - 여백(전체 또는 상/우/하/좌 개별 지정)
  - 코드 하이라이트 on/off
  - 목차 on/off(선택)
  - 스타일 프리셋(`default`, `github`, `docs`, `plain` 등)
  - 사용자 CSS 오버라이드
  - 헤더/푸터 템플릿
  - 페이지 번호/총 페이지/문서 제목/생성일 토큰
  - 페이지 나눔 제어(섹션 시작, 표/코드블록 분할 방지 우선 정책)
  - PDF 메타데이터(title/author/subject)
  - 디버그 산출물 생성 on/off
  - mermaid 모드(`off`, `auto`, `on`)
  - math 모드(`off`, `auto`, `on`)

### 7.2 WASM 모듈
- 입력: ZIP(ArrayBuffer), 옵션 JSON
- 동작:
  - ZIP 해제 (브라우저 메모리 내)
  - 안전한 경로 필터링(`..`, 절대경로, 드라이브 경로 차단)
  - md 후보 목록 생성 및 단일/일괄 선택 지원
  - md/이미지/raw HTML `<img>` 연결 검증
  - Mermaid 블록 렌더(모드별 동작):
    - `off`: 렌더 생략
    - `auto`: 렌더 시도, 실패 시 코드블록 유지 + warning
    - `on`: 렌더 실패 시 error
  - Math 블록 렌더(모드별 동작):
    - `off`: 렌더 생략
    - `auto`: 렌더 시도, 실패 시 원문 유지 + warning
    - `on`: 렌더 실패 시 error
  - 렌더 가능한 HTML + 진단 리포트 반환
- 출력:
  - HTML 문자열(단일) 또는 HTML 목록(일괄)
  - 진단 정보(JSON): missing assets, ignored files, warnings, mermaid diagnostics, math diagnostics
  - 선택적 디버그 산출물: merged HTML, asset manifest, render report

### 7.3 CLI
- 명령 예시:
  - `marknest convert`
  - `marknest convert README.md -o README.pdf`
  - `marknest convert README.md --mermaid auto -o README.pdf`
  - `marknest convert README.md --math auto -o README.pdf`
  - `marknest convert README.md --theme github -o README.pdf`
  - `marknest convert README.md --css ./pdf.css --header-template ./header.html --footer-template ./footer.html -o README.pdf`
  - `marknest convert README.md --landscape --margin-top 24 --margin-right 16 --margin-bottom 24 --margin-left 16 -o README.pdf`
  - `marknest convert README.md --debug-html ./out/debug.html --asset-manifest ./out/assets.json -o README.pdf`
  - `marknest convert ./docs.zip --entry docs/README.md -o out.pdf`
  - `marknest convert ./docs.zip --all --out-dir ./pdf`
  - `marknest convert ./docs.zip --all --config ./marknest.toml --render-report ./out/report.json --out-dir ./pdf`
  - `marknest convert ./docs --out-dir ./pdf`
  - `marknest validate README.md --strict`
  - `marknest validate README.md --mermaid on --strict`
  - `marknest validate README.md --math on --strict`
  - `marknest validate README.md --config ./marknest.toml --render-report ./out/report.json --strict`
  - `marknest validate ./docs.zip --entry docs/README.md --strict`
  - `marknest validate ./docs --all --report report.json`
- 기능:
  - 입력 타입 자동 인식 (`.zip`, `.md`, 폴더, 미지정)
  - 기본 동작은 현재 경로(`.`) 기준 변환
  - 인자 미지정 시 엔트리 md 자동 탐지 (`README.md` -> `index.md` -> 단일 md 파일)
  - md 파일 위치 기준으로 상대경로 이미지/raw HTML `<img>` 자동 해석 (`--assets` 불필요)
  - ZIP/폴더 입력 시 재귀 탐색 기반 동일 파이프라인 사용
  - 다중 md 처리:
    - `--entry <path>`: 단일 변환
    - `--all`: 일괄 변환
    - 다중 md 감지 + 옵션 미지정 시 명확한 가이드와 함께 실패
  - 폴더 입력 시 기본 동작은 재귀 일괄 변환 (`--all` 암묵 적용)
  - `--entry`, `--cwd`, `-o/--output`, `--out-dir`, `--mermaid`, `--math` 옵션 제공
  - `--theme`, `--css`, `--header-template`, `--footer-template` 옵션 제공
  - `--landscape`, `--margin-top`, `--margin-right`, `--margin-bottom`, `--margin-left` 옵션 제공
  - `--title`, `--author`, `--subject` 옵션으로 PDF 메타데이터 지정
  - `--debug-html`, `--asset-manifest`, `--render-report` 옵션으로 디버그 산출물 저장
  - `--config <path>` 옵션과 기본 설정 파일 탐색(`.marknest.toml`, `marknest.toml`) 지원
  - 환경변수(`MARKNEST_CONFIG`, `MARKNEST_THEME`, `MARKNEST_CSS`)로 기본값 주입 지원
  - 옵션 우선순위는 `CLI 인자 > 설정 파일 > 환경변수 > 내장 기본값`
  - `validate` 모드 (PDF 생성 없이 링크/구조 검증)
  - CI 친화적 exit code
  - 일괄 변환 시 상대 경로 보존 출력 (`docs/a/b.md` -> `out/a/b.pdf`)
  - `--mermaid` 기본값은 `auto`
  - `--math` 기본값은 `auto`

### 7.4 웹 애플리케이션
- 업로드 방식:
  - Drag & Drop
  - 파일 선택
  - `.zip` 전용 지원 (MVP 필수)
- UX:
  - 업로드 직후 구조 분석 결과 표시
  - md 후보 목록 + 단일/일괄 선택 UI
  - 누락 이미지 경고 표시
  - raw HTML `<img>` 누락 경고 표시
  - 스타일 프리셋 선택 UI
  - 사용자 CSS 업로드 또는 편집 UI
  - 헤더/푸터 템플릿 입력 및 미리보기 UI
  - 페이지 크기/방향/여백 설정 UI
  - Mermaid 렌더 옵션 토글(`off`/`auto`/`on`)
  - Mermaid 렌더 실패 시 다이어그램 단위 경고 표시
  - Math 렌더 옵션 토글(`off`/`auto`/`on`)
  - Math 렌더 실패 시 수식 단위 경고 표시
  - PDF 메타데이터(title/author/subject) 입력 UI
  - PDF 생성 버튼 + 진행 상태
  - 디버그 번들(HTML + manifest + diagnostics) 다운로드 제공
  - 일괄 변환 결과 ZIP 다운로드 제공
- 렌더링 전략:
  - 1차: 클라이언트(WASM) 기반 렌더/미리보기
  - 기본 모드는 `Browser Fast`이며, 업로드된 ZIP/Markdown 본문을 MarkNest backend로 전송하지 않는다
  - 2차: 품질 요구 또는 실패 시에만 fallback 서버 PDF 렌더를 사용한다
  - 문서가 참조한 외부 HTTP(S) 이미지/에셋은 브라우저 또는 네이티브 런타임이 직접 fetch할 수 있다
  - 단일 `.md` 직접 업로드는 비MVP

### 7.5 Mermaid 지원 정책
- 렌더 포맷: SVG 우선 (PDF 품질/확대 대응)
- 보안 기본값: Mermaid `securityLevel: strict`
- MarkNest가 제공하는 Mermaid 런타임 자산은 외부 CDN에 의존하지 않고 로컬 번들 자산을 사용한다
- 실패 fallback:
  - `auto`: 원본 code block 유지 + warning
  - `on`: 변환 실패(error)
- CLI/Web 동일 옵션 의미를 보장

### 7.6 Math 및 Raw HTML img 지원 정책
- 수식 엔진 기본값: MathJax (MVP 고정)
- 지원 대상:
  - Markdown 수식(inline/block)
  - raw HTML `<img src=\"...\">`
- 지원 제외(기본):
  - raw HTML `<video>`, `<audio>`, `<iframe>`
  - 실행형 스크립트/동적 DOM
- 수식 처리:
  - `auto`: 정적 렌더 시도, 실패 시 원문 유지 + warning
  - `on`: 렌더 실패 시 변환 실패(error)
- raw HTML `<img>` 처리:
  - 상대/절대 경로 resolve 및 존재 여부 검증
  - `http://`, `https://` 원격 이미지 링크는 품질 향상을 위해 직접 fetch 및 inlining을 시도할 수 있다
  - 미지원 스킴(`javascript:` 등) 차단

### 7.7 스타일 및 인쇄 레이아웃 정책
- 스타일 시스템:
  - 기본 스타일시트 위에 프리셋 테마와 사용자 CSS를 순차 적용
  - 적용 우선순위는 `base stylesheet -> theme preset -> user CSS`
  - 프리셋 테마는 문서형(`docs`), GitHub형(`github`), 최소형(`plain`)을 기본 제공
- 헤더/푸터:
  - HTML 템플릿 기반으로 정의
  - 지원 토큰: `pageNumber`, `totalPages`, `title`, `date`, `entryPath`
  - 보안상 스크립트 실행은 금지하고 sanitize를 적용
- 인쇄 레이아웃:
  - portrait/landscape 전환을 지원
  - 상/우/하/좌 개별 여백을 지원
  - 제목 시작 전 페이지 나눔, 표/코드블록 분할 방지 우선 정책을 지원
  - 브라우저 미리보기와 최종 PDF 간 레이아웃 차이를 최소화한다
- 메타데이터:
  - PDF `title`, `author`, `subject`를 지정 가능해야 한다

### 7.8 설정, 디버그, 실행 환경 정책
- 설정 파일:
  - `.marknest.toml` 또는 `marknest.toml` 자동 탐색을 지원한다
  - 프로젝트 공통 preset과 출력 규칙을 설정 파일로 저장할 수 있어야 한다
- 환경변수:
  - CI/배치 실행용 기본 설정 주입을 지원한다
  - 설정 우선순위는 `CLI 인자 > 설정 파일 > 환경변수 > 내장 기본값`으로 고정한다
- 디버그 산출물:
  - merged HTML, asset manifest, render report를 선택적으로 저장할 수 있어야 한다
  - 디버그 산출물은 재현 가능한 오류 분석에 필요한 최소 정보를 포함해야 한다
- 실행 환경:
  - CLI와 서버 fallback용 공식 Docker 이미지를 제공한다
  - Chromium, Mermaid, MathJax, 기본 폰트 버전을 고정하거나 명시적으로 관리한다
  - 공식 실행 환경의 MarkNest 자체 런타임 자산(Mermaid, MathJax, html2pdf 등)은 외부 CDN 없이 동작해야 한다
  - 다만 문서가 참조하는 외부 HTTP(S) 이미지/에셋 fetch는 허용 범위이며, 이 경우 완전한 no-network 동작은 보장 범위가 아니다
  - 진단 리포트에 실행 엔진/버전 정보를 포함한다

## 8. 비기능 요구사항

### 8.1 성능
- 20MB 이하 ZIP: 업로드 후 3초 내 분석 시작
- 100페이지 이하 문서: 평균 10초 내 PDF 생성(표준 환경)
- CLI 폴더 일괄 변환: md 100개 기준 진행률/요약 로그 제공
- Mermaid 렌더 타임아웃(다이어그램 1개당) 기본 5초, 설정 가능
- Math 렌더 타임아웃(수식 블록 1개당) 기본 3초, 설정 가능

### 8.2 안정성
- 변환 성공/실패 사유를 사용자에게 명확히 표시
- 실패 시 fallback 경로 제공

### 8.3 보안
- Zip Slip 방지 (경로 탈출 금지)
- 허용 확장자 정책 (`.md`, `.markdown`, 이미지 확장자 등)
- 압축폭탄 방지:
  - 압축 해제 후 총 용량 상한
  - 파일 개수 상한
- 악성 스크립트 삽입 대응:
  - HTML sanitize 옵션
  - PDF 렌더 단계에서 외부 리소스 차단 옵션(선택 기능)

### 8.4 호환성
- 데스크톱 최신 Chrome/Edge/Safari/Firefox
- 모바일은 미리보기 중심, 대용량 변환은 제한 안내

### 8.5 운영 및 재현성
- 공식 Docker 이미지 기준으로 로컬/CI/서버 fallback 실행이 가능해야 한다
- 동일 입력/옵션/공식 런타임에서는 페이지 수와 주요 레이아웃이 재현 가능해야 한다
- 서버 fallback 런타임은 Chromium, Mermaid, MathJax, 폰트 버전을 릴리즈 노트에 명시해야 한다
- MarkNest 자체 런타임 자산은 외부 네트워크 없이도 기본 렌더 경로가 동작해야 한다
- 단, 문서가 참조하는 외부 HTTP(S) 이미지/에셋 fetch는 fidelity 향상을 위해 허용될 수 있으며, 이는 backend 무업로드 정책과 별개로 취급한다

## 9. 시스템 아키텍처
- `core` (Rust crate): 파서/리졸버/진단/옵션
- `wasm` (Rust -> wasm32): 브라우저 실행 계층
- `cli` (Rust binary): 현재 경로 md + ZIP 자동 변환/검증 도구
- `web`:
  - Frontend: ZIP 업로드/미리보기/결과 다운로드
  - Backend (선택적): Chromium 기반 printToPDF fallback

데이터 흐름:
1. Web: 클라이언트가 ZIP 업로드
2. Web: WASM에서 해제/엔트리 인덱싱/검증/HTML 생성
3. Web: 단일 또는 일괄 렌더 요청
4. Web: 브라우저 PDF 생성 시도 (기본 경로, backend 업로드 없음)
5. Web: 실패 또는 고품질 모드 선택 시에만 서버 렌더 요청
6. CLI: 현재 경로/지정 md/ZIP/폴더를 읽고 단일 또는 일괄 PDF 생성

## 10. API/인터페이스 초안

### 10.1 Rust Core API (초안)
```rust
pub struct ConvertOptions {
    pub page_size: PageSize,
    pub orientation: PageOrientation,
    pub margins_mm: PageMarginsMm,
    pub enable_toc: bool,
    pub sanitize_html: bool,
    pub theme: Option<String>,
    pub custom_css: Option<String>,
    pub header_template: Option<HeaderFooterTemplate>,
    pub footer_template: Option<HeaderFooterTemplate>,
    pub metadata: PdfMetadata,
    pub emit_debug_artifacts: bool,
    pub mermaid_mode: MermaidMode,
    pub math_mode: MathMode,
}

pub struct Diagnostic {
    pub missing_assets: Vec<String>,
    pub warnings: Vec<String>,
    pub ignored_files: Vec<String>,
    pub mermaid_warnings: Vec<String>,
    pub mermaid_errors: Vec<String>,
    pub math_warnings: Vec<String>,
    pub math_errors: Vec<String>,
    pub raw_html_img_warnings: Vec<String>,
    pub config_warnings: Vec<String>,
    pub runtime_info: Vec<String>,
}

pub fn analyze_zip(bytes: &[u8]) -> Result<ProjectIndex, Error>;
pub fn analyze_workspace(root: &Path) -> Result<ProjectIndex, Error>;
pub fn render_html(project: &ProjectIndex, entry: &str, opt: &ConvertOptions) -> Result<(String, Diagnostic), Error>;
pub fn render_html_batch(project: &ProjectIndex, entries: &[String], opt: &ConvertOptions) -> Result<Vec<(String, String, Diagnostic)>, Error>;
```

### 10.2 WASM API (초안)
```ts
analyzeZip(zipBytes: Uint8Array): ProjectIndex
renderHtml(entryPath: string, options: ConvertOptions): { html: string; diagnostic: Diagnostic }
renderHtmlBatch(entryPaths: string[], options: ConvertOptions): Array<{ entry: string; html: string; diagnostic: Diagnostic }>
renderDebugBundle(entryPath: string, options: ConvertOptions): { html: string; manifest: Uint8Array; report: Uint8Array }
```

### 10.3 CLI UX (초안)
```bash
marknest convert
marknest convert README.md -o README.pdf
marknest convert README.md --mermaid auto -o README.pdf
marknest convert README.md --math auto -o README.pdf
marknest convert README.md --theme github -o README.pdf
marknest convert README.md --css ./pdf.css --header-template ./header.html --footer-template ./footer.html -o README.pdf
marknest convert README.md --landscape --margin-top 24 --margin-right 16 --margin-bottom 24 --margin-left 16 -o README.pdf
marknest convert README.md --debug-html ./out/debug.html --asset-manifest ./out/assets.json -o README.pdf
marknest convert ./docs.zip --entry docs/README.md -o out.pdf
marknest convert ./docs.zip --all --out-dir ./pdf
marknest convert ./docs.zip --all --config ./marknest.toml --render-report ./out/report.json --out-dir ./pdf
marknest convert ./docs --out-dir ./pdf
marknest convert --cwd ./docs --entry guide/getting-started.md -o out.pdf
marknest validate README.md --strict
marknest validate README.md --mermaid on --strict
marknest validate README.md --math on --strict
marknest validate README.md --config ./marknest.toml --render-report ./out/report.json --strict
marknest validate ./docs.zip --entry docs/README.md --strict
marknest validate ./docs --all --report report.json
```

## 11. 에러 처리 정책
- 사용자 에러:
  - "현재 경로에서 엔트리 md 파일을 찾을 수 없습니다"
  - "엔트리 md 파일을 찾을 수 없습니다"
  - "ZIP 내부에서 엔트리 md 파일을 찾을 수 없습니다"
  - "다중 md가 감지되었습니다. `--entry` 또는 `--all`을 지정하세요"
  - "출력 파일 경로가 충돌합니다. `--out-dir`를 지정하세요"
  - "다음 이미지 파일이 누락되었습니다: ..."
  - "Mermaid 렌더링에 실패했습니다: <diagram-id>"
  - "Math 렌더링에 실패했습니다: <formula-id>"
  - "raw HTML img 링크가 유효하지 않습니다: <src>"
  - "스타일시트 파일을 읽을 수 없습니다: <path>"
  - "헤더/푸터 템플릿이 유효하지 않습니다: <path>"
  - "설정 파일을 해석할 수 없습니다: <path>"
  - "디버그 산출물 경로가 충돌합니다: <path>"
- 시스템 에러:
  - "PDF 엔진 실패, 서버 fallback을 시도하세요"
- 진단 레벨:
  - `error`: 변환 중단
  - `warning`: 변환 가능, 품질 저하 가능성 표시

## 12. 품질 기준 (Acceptance Criteria)
- ZIP 내부 상대경로 이미지가 95% 이상 케이스에서 정상 렌더
- CLI 현재 경로 변환에서 상대경로 이미지가 95% 이상 케이스에서 정상 렌더
- CLI ZIP 변환에서 상대경로 이미지가 95% 이상 케이스에서 정상 렌더
- ZIP 다중 md에서 단일/일괄 선택이 의도대로 동작
- CLI 폴더 입력 시 하위 md 파일이 누락 없이 재귀 변환
- Mermaid 코드블록이 `auto/on` 모드에서 SVG로 렌더되거나, 정책대로 fallback/실패 처리
- 수식(Math)이 `auto/on` 모드에서 렌더되거나, 정책대로 fallback/실패 처리
- raw HTML `<img src>`가 경로 해석/검증 규칙에 따라 렌더 또는 경고 처리
- `validate`가 누락 파일/경로 오류를 재현 가능하게 보고
- 브라우저 렌더 실패 시 fallback으로 PDF 생성 가능
- 보안 테스트에서 경로 탈출 샘플 ZIP 차단
- 프리셋 테마와 사용자 CSS가 정의된 우선순위대로 일관되게 적용된다
- 헤더/푸터 템플릿의 토큰이 PDF에서 의도한 값으로 치환된다
- 방향 전환과 개별 여백 설정이 브라우저 렌더와 서버 fallback 양쪽에서 일관되게 반영된다
- `--debug-html`, `--asset-manifest`, `--render-report`가 재현 가능한 디버그 산출물을 생성한다
- 설정 파일/환경변수/CLI 인자의 우선순위가 문서화된 규칙대로 동작한다
- 공식 Docker 이미지 기준으로 CLI와 서버 fallback이 동일한 기본 출력 정책을 제공한다

## 13. 측정 지표 (KPI)
- 변환 성공률
- 평균 처리 시간 (업로드~다운로드)
- 누락 이미지 발생률
- fallback 사용률
- 재시도율
- 일괄 변환 성공률 (파일 단위)
- Mermaid 렌더 성공률
- Math 렌더 성공률
- raw HTML img 유효성 오류율

## 14. MVP 범위
- 본 PRD의 MVP는 한 번에 전체 범위를 포함하지 않고, 아래 두 단계로 나누어 정의한다.

### 14.1 Engineering MVP (권장: Phase 0 ~ Phase 4)
- Rust core 기반 workspace/ZIP 분석과 진단 JSON 생성
- CLI `validate` 지원:
  - `.md`, `.zip`, 폴더 입력
  - 엔트리 탐지
  - 상대경로 이미지/raw HTML `<img>` 검증
  - `--strict`, report 출력, CI 친화적 exit code
- CLI `convert` 지원:
  - 단일 `.md` 변환
  - ZIP 단일 엔트리 변환
  - 폴더/ZIP 일괄 변환
  - 상대경로 이미지 포함 PDF 생성
- 기본 출력 옵션:
  - page size
  - orientation
  - margin
  - 기본 theme preset
- 일괄 변환 시 상대 경로 보존 출력 및 충돌 감지
- Web UI, 서버 fallback, 고급 스타일링, Mermaid/Math 고급 옵션은 Engineering MVP 범위에서 제외

### 14.2 External Beta (권장: Phase 5 ~ Phase 6)
- Web ZIP 업로드 전용
- Web 구조 분석, 엔트리 선택, 누락 에셋 경고, HTML preview
- Web 브라우저 PDF 생성
- Web 단일/일괄 다운로드
- Mermaid/Math는 `off` 또는 제한적 `auto`로 시작 가능
- 서버 fallback, 고급 템플릿, Docker 배포 표준화는 Beta 이후 단계로 이관

### 14.3 GA 후보 범위 (권장: Phase 7 ~ Phase 8)
- 서버 fallback(Playwright/Chromium)
- Mermaid/Math `auto/on` 정책 완성
- 스타일 프리셋/사용자 CSS/헤더/푸터/메타데이터/디버그 번들 완성
- 설정 파일/환경변수/공식 Docker/runtime pinning 제공

## 15. 릴리즈 단계 제안
- Phase 0: Core Analyze Foundation
  - Rust core에서 workspace/ZIP 읽기, 엔트리 탐지, 상대경로 에셋 해석, 진단 JSON 생성
  - PDF 생성은 아직 포함하지 않음
  - 완료 조건:
    - `analyze_workspace`와 `analyze_zip`가 공통 `ProjectIndex`/진단 구조를 반환한다
    - 누락 에셋, 무시된 파일, 엔트리 후보 목록이 재현 가능한 JSON 형태로 출력된다
    - 경로 탈출(`..`, 절대경로, 드라이브 경로) 샘플이 차단된다
  - 권장 구현 티켓:
    - P0-1. `ProjectIndex`, `EntryCandidate`, `AssetRef`, `Diagnostic`, `Error` 모델 정의
    - P0-2. 실제 FS/메모리 FS를 공통 인터페이스로 다루는 파일 시스템 추상화 구현
    - P0-3. 경로 정규화, allowlist 검사, Zip Slip 차단 유틸리티 구현
    - P0-4. workspace 탐색기 구현:
      - `.md`, `.markdown`, 이미지 확장자 스캔
      - 무시 파일/디렉터리 처리
    - P0-5. ZIP 분석기 구현:
      - 안전한 엔트리 필터링
      - 메모리 내 인덱싱
      - 파일 수/해제 크기 제한
    - P0-6. 엔트리 탐지 규칙 구현:
      - `README.md`
      - `index.md`
      - 단일 md
      - 다중 md 충돌 상태
    - P0-7. Markdown 이미지와 raw HTML `<img>` 자산 해석기 구현
    - P0-8. JSON 직렬화 및 fixture 기반 golden test 추가
      - 정상 workspace
      - 누락 이미지
      - 악성 ZIP
      - 다중 엔트리

- Phase 1: CLI Validate
  - `validate` 명령으로 `.md`, `.zip`, 폴더 입력 검증
  - `--strict`, report 출력, 누락 에셋/엔트리 오류/경로 오류 진단
  - CI용 exit code 확정
  - 완료 조건:
    - `validate`가 `.md`, `.zip`, 폴더 입력 모두에서 동작한다
    - 성공/경고/실패가 정의된 exit code로 구분된다
    - report에 엔트리 탐지 결과, 누락 에셋, warning/error가 포함된다
  - 권장 구현 티켓:
    - P1-1. CLI 명령 구조 스캐폴딩:
      - `validate`
      - 공통 옵션 파서
      - `--help`
    - P1-2. 입력 타입 판별기 구현:
      - 인자 미지정 시 현재 경로
      - `.md`
      - `.zip`
      - 폴더
    - P1-3. `validate`와 core 분석기 연결
      - `--entry`
      - `--all`
      - `--strict`
      - `--report`
    - P1-4. 진단 포맷터 구현:
      - 사람용 콘솔 출력
      - JSON report 출력
      - warning/error 요약
    - P1-5. exit code 정책 구현:
      - 성공
      - warning 포함 성공
      - 검증 실패
      - 시스템 실패
    - P1-6. 대표 사용자 에러 메시지 확정
      - 엔트리 없음
      - 다중 md 감지
      - 누락 에셋
      - 잘못된 경로
    - P1-7. CLI 통합 테스트 추가:
      - 단일 md validate
      - zip validate
      - folder validate
      - `--strict` 동작
      - report 파일 생성

- Phase 2: CLI Single Convert Basic
  - 현재 경로 또는 지정 `.md` 단일 변환
  - raw HTML `<img>` 포함 기본 이미지 렌더
  - PDF 생성 경로와 기본 page size/margin 지원
  - Mermaid/Math는 `off` 또는 미지원
  - 완료 조건:
    - `marknest convert`와 `marknest convert README.md`가 단일 PDF를 생성한다
    - 상대경로 Markdown 이미지와 raw HTML `<img>`가 정상 포함되거나 명확한 경고를 출력한다
    - page size/margin 옵션이 결과 PDF에 반영된다

- Phase 3: CLI Single Convert Advanced
  - Mermaid `auto` 도입
  - Math `auto` 도입
  - orientation, 기본 theme preset, 메타데이터 등 단일 문서 품질 옵션 추가
  - 렌더 실패 시 warning/error 정책 확정
  - 완료 조건:
    - `--mermaid auto`와 `--math auto`가 성공 시 렌더하고 실패 시 warning으로 fallback한다
    - orientation, theme preset, PDF 메타데이터가 결과물에 반영된다
    - `auto`와 `on`의 실패 정책 차이가 CLI 출력과 exit code에서 일관되게 드러난다

- Phase 4: CLI Batch and Workspace
  - ZIP 단일 엔트리 변환
  - ZIP/폴더 `--all` 일괄 변환
  - 상대 경로 보존 출력, 파일명 충돌 처리, batch summary
  - 이 단계 완료 시 Engineering MVP 달성
  - 완료 조건:
    - `convert input.zip --entry ...`와 `convert input.zip --all`이 의도한 출력 파일을 생성한다
    - 폴더 입력 시 하위 `.md`가 누락 없이 재귀 처리되고 경로 구조가 보존된다
    - 파일명 충돌, 실패 파일, 누락 에셋이 batch summary/report에 포함된다

- Phase 5: Web Analyze and Preview
  - ZIP 업로드
  - WASM 기반 분석
  - 엔트리 선택 UI, 누락 에셋 경고, HTML preview
  - PDF 생성 없이 preview 중심으로 검증
  - 완료 조건:
    - 사용자가 ZIP 업로드 후 md 후보 목록과 누락 에셋 경고를 확인할 수 있다
    - 선택한 엔트리의 HTML preview가 이미지 포함 상태로 렌더된다
    - 분석과 preview 경로가 서버 없이 클라이언트(WASM)만으로 동작한다

- Phase 6: Web Browser PDF
  - 브라우저 기반 단일 PDF 생성
  - 일괄 변환 결과 ZIP 다운로드
  - 이 단계 완료 시 External Beta 달성
  - 완료 조건:
    - 선택한 단일 엔트리에 대해 브라우저에서 PDF 다운로드가 가능하다
    - 일괄 변환 결과가 ZIP으로 다운로드된다
    - Web 경로에서 기본 진단 정보와 진행 상태가 사용자에게 표시된다

- Phase 7: Server Fallback and Scale
  - Playwright/Chromium 기반 fallback
  - 대용량 ZIP 가드 및 품질 모드
  - 브라우저 실패 시 서버 경로 전환 정책 확정
  - 완료 조건:
    - 브라우저 PDF 생성 실패 또는 품질 모드 선택 시 서버 fallback이 동작한다
    - 대용량 ZIP 입력에서 제한 안내 또는 서버 모드 유도가 일관되게 표시된다
    - fallback 결과에도 엔트리 선택, 이미지 자산, 기본 진단 정보가 유지된다

- Phase 8: Output Quality and Operations
  - 스타일 프리셋, 사용자 CSS, 헤더/푸터, 페이지 번호, 디버그 번들
  - 설정 파일/환경변수, 공식 Docker 이미지, runtime pinning
  - 재현성, 폰트 정책, 운영 관측성 고도화
  - 완료 조건:
    - 스타일 프리셋/사용자 CSS/헤더/푸터/페이지 번호가 정의된 우선순위대로 반영된다
    - `--config`, 환경변수, CLI 인자의 우선순위가 문서화된 규칙대로 동작한다
    - 디버그 번들(HTML, asset manifest, render report)이 재현 가능한 형태로 생성된다
    - 공식 Docker 이미지 기준으로 CLI와 서버 fallback이 동일한 기본 런타임 정보를 보고한다

## 16. 리스크 및 대응
- 리스크: 브라우저 메모리 한계로 대용량 ZIP 실패
  - 대응: 용량 가드 + 서버 모드 유도
- 리스크: 폰트/한글 렌더 품질 편차
  - 대응: 서버 엔진 표준화 + 폰트 번들 정책
- 리스크: 악성 ZIP 업로드
  - 대응: 해제 제한/확장자 allowlist/샌드박스 처리

## 17. 오픈 이슈
- 서버 fallback을 기본 ON으로 둘지, 품질 모드 선택형으로 둘지
- CLI 엔트리 자동 탐지 우선순위(`README.md`, `index.md`, 기타`)를 고정할지 설정화할지
- 일괄 변환 결과 파일명 충돌 시 suffix 규칙(`-1`, `-2` 등) 정책
- Mermaid 렌더 엔진 버전 고정 정책(재현성 vs 최신 기능)
- Mermaid 테마/폰트를 PDF 기본 스타일과 얼마나 동기화할지
- MathJax 버전 고정 정책(재현성 vs 최신 기능)
- 무료 플랜에서 파일/용량 제한 정책
- 장기적으로 로컬 전용(PWA) 모드 제공 여부
- 기본 제공 테마 프리셋 목록과 유지보수 범위
- 헤더/푸터 토큰 확장 범위와 커스텀 템플릿 sandbox 규칙
- 공식 Docker 이미지의 배포 방식(단일 이미지 vs CLI/서버 분리)

## 18. 초기 기술 스택 제안
- Rust crates:
  - Markdown: `pulldown-cmark` 또는 `comrak`
  - ZIP: `zip`
  - 경로 처리: `camino`
  - WASM 바인딩: `wasm-bindgen`
  - Mermaid 연동: `mermaid` JS 엔진(웹), CLI는 headless JS 런타임 연계
  - Math 연동: `mathjax` 정적 렌더러
- Web:
  - Frontend: React/Vue/Svelte 중 택1
  - PDF fallback: Playwright + Chromium
  - 배포: 공식 Docker 이미지, 폰트 번들, 오프라인 자산 패키징

## 19. 결론
본 PRD는 "ZIP 기반 Markdown+이미지 PDF 변환"이라는 명확한 문제를 해결하기 위한 제품 요구사항을 정의한다. 핵심 전략은 Rust 코어 재사용성과 WASM 클라이언트 처리, 그리고 고품질 보장을 위한 서버 fallback의 조합이다.
