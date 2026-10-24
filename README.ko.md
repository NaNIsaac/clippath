<img src="assets/logo.png" width="96" align="right" alt="">

# clippath

**한 번 캡처하고, 필요한 곳에 붙여넣으세요.**

[English](README.md)

Windows 클립보드에 있는 이미지를 PNG 파일로 만들고 그 경로를 클립보드에 넣습니다.
그래서 Windows나 WSL의 터미널 AI 에이전트(Claude Code, Codex 등)에 스크린샷을 바로
붙여넣을 수 있습니다. 이미지도 클립보드에 그대로 남아 있어서, 그림판이나 메신저에
붙여넣으면 이미지가 붙습니다.

![가격 카드를 캡처해 Claude Code에 붙여넣으면 같은 카드를 HTML로 만들어 주는 장면](assets/claude-code.gif)

![스크린샷을 찍으면 clippath가 클립보드에 파일 경로를 더하고, Ctrl+V 하면 터미널에는 경로가, 다른 앱에는 이미지가 붙는 흐름](assets/how-it-works.svg)

## 설치

PowerShell(Windows PowerShell 5.1 또는 PowerShell 7)에서:

<!-- test windows -->
```powershell
irm https://raw.githubusercontent.com/NaNIsaac/clippath/main/install.ps1 | iex
```

[`install.ps1`](install.ps1)은 `clippath-<버전>-windows-x64.zip` 파일을 정확한 이름으로 골라
SHA-256을 확인한 뒤(릴리스의 `.sha256` 파일, 예전 릴리스는 GitHub의 asset digest),
`clippath.exe`와 `clippathw.exe`를 `%LOCALAPPDATA%\Programs\clippath`에 넣고 그 폴더를 사용자 PATH에
한 번만 추가합니다. 다운로드나 체크섬 확인이 실패하면 기존 설치는 그대로 둡니다.

- **업데이트**: [업데이트](#업데이트)를 보세요.
- **버전 고정**: 먼저 `$env:CLIPPATH_VERSION = 'v0.5.0'`.
- **다른 폴더**: 먼저 `$env:CLIPPATH_INSTALL_DIR = 'D:\Tools\clippath'`.
- **삭제**: `& ([scriptblock]::Create((irm https://raw.githubusercontent.com/NaNIsaac/clippath/main/install.ps1))) -Uninstall`.
  감시를 멈추고 로그인 자동 실행과 PATH에서 빼고
  exe를 지웁니다. `%LOCALAPPDATA%\clippath`에 저장된 스크린샷은 남깁니다. 지우려면 삭제 전에
  `clippath clean`을 실행하세요.

실행 전에 [스크립트를 읽어볼](install.ps1) 수 있고, [최신 릴리스](https://github.com/NaNIsaac/clippath/releases/latest)에서
zip을 받아 exe 두 개를 PATH의 한 폴더에 직접 넣어도 됩니다.

끝나면 터미널을 새로 여세요. 명령은 `clippath.exe`이고, `clippathw.exe`는 콘솔 창 없이 도는 같은
감시 프로그램으로 `clippath start`가 로그인할 때 실행합니다. 두 파일은 같은 폴더에 두세요.

설치가 됐는지 확인하세요:

<!-- test windows -->
```powershell
clippath --version
```

## 업데이트

설치 명령을 다시 실행하세요. 최신 릴리스를 받아 SHA-256을 확인하고, `%LOCALAPPDATA%\Programs\clippath`의
exe 두 개를 바꾸고, 백그라운드 감시가 켜져 있으면 새 버전으로 다시 시작합니다. 한 일을 출력합니다.
예: `clippath: updated 0.5.0 -> 0.6.0`. 아무것도 출력되지 않았다면 실행되지 않은 것입니다.

<!-- test windows -->
```powershell
irm https://raw.githubusercontent.com/NaNIsaac/clippath/main/install.ps1 | iex
```

터미널을 새로 열어 버전을 확인하세요:

<!-- test windows -->
```powershell
clippath --version
```

그래도 0.4.0이 나오면 예전 Python 패키지의 복사본이 PATH에서 먼저 잡히는 것입니다(설치 스크립트가
경고합니다). 그 복사본을 지우고, 터미널을 새로 열고, 어느 `clippath`가 먼저 실행되는지 확인하세요:

```powershell
uv tool uninstall clippath    # 또는: pip uninstall clippath
Get-Command clippath -All     # 첫 줄이 Programs\clippath\clippath.exe로 끝나야 합니다
```

## 사용법

한 번만 실행하세요:

```powershell
clippath start          # 백그라운드 감시 시작, 로그인할 때마다 자동 실행
clippath start --wsl    # 같은 기능, /mnt/c/... 경로로 복사
```

그다음에는 캡처하고(`Win+Shift+S`, PrintScreen, 어떤 도구든) AI 터미널에서 `Ctrl+V`
하면 됩니다:

```text
> 이 레이아웃 뭐가 문제야? "C:\Users\me\Pictures\Screenshots\스크린샷 2026-10-16 091500.png"
```

경로는 쌍따옴표로 감싸서 띄어쓰기가 있어도 안전합니다. 경로는 터미널, 코드 에디터,
메모장, 탐색기에만 붙고, 브라우저(ChatGPT 등), 메신저, 그림판에는 이미지만 붙습니다.
일반 텍스트 복사는 건드리지 않습니다.

그냥 경로라서 어떤 명령에든 쓸 수 있습니다:

![캡처한 뒤 PowerShell에서 Ctrl+V 하면 cp 명령에 따옴표로 감싼 파일 경로가 붙는 장면](assets/terminal.gif)

실행 중에는 트레이에 clippath 아이콘이 보입니다(클릭하면 폴더 열기, 종료).

## 명령어

| 명령 | 하는 일 |
|---|---|
| `clippath start [--wsl]` | 백그라운드 감시 시작, 로그인할 때마다 자동 실행 |
| `clippath stop` | 감시 종료, 자동 실행 해제 |
| `clippath status` | 감시 상태, 자동 실행 여부, 저장된 파일 정보 |
| `clippath watch [--wsl]` | 터미널 창에서 감시 (Ctrl+C로 종료) |
| `clippath copy [--wsl]` | 지금 클립보드 이미지를 한 번만 변환 |
| `clippath clean [--days N]` | 저장한 스크린샷 삭제 (최근 N일 치는 남김) |

`clippath -h`, `clippath <명령> -h`, `clippath --version`.

## 파일은 어디에 저장되나요

캡처 도구가 이미 파일을 저장했다면(캡처 도구, `Win+PrtScn` → `사진\Screenshots`)
따로 복사본을 만들지 않고 그 파일의 경로를 복사합니다. 이때 파일의 픽셀이 클립보드 이미지와
같은지 먼저 확인합니다. 그 밖의 이미지는
`%LOCALAPPDATA%\clippath\`에 SHA-256 이름으로 저장해서, 같은 이미지는 한 번만 저장합니다.
`clippath clean`은 이 폴더만 지웁니다. 어떤 데이터도 외부로 보내지 않습니다.

## 소스에서 빌드하기

```bash
cargo build --release   # target\release\clippath.exe, clippathw.exe
cargo test
```
