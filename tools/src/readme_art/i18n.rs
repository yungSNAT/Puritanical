// SPDX-License-Identifier: Apache-2.0 AND MIT

//! The panels' strings, one `Lang` per README translation.

use super::{Lang, n};

pub(super) const LANGS: [Lang; 5] = [EN, ES, KO, ZH_CN, JA];

fn en_words(a: usize, b: usize) -> String {
    format!("{a} words → {b} words")
}
fn en_cut(p: f64) -> String {
    format!("−{}% vs no ruleset", n(p))
}
fn en_hit(k: usize, a: &str) -> String {
    format!("✓ rung {k}: {a}")
}
fn es_words(a: usize, b: usize) -> String {
    format!("{a} palabras → {b}")
}
fn es_cut(p: f64) -> String {
    format!("−{}% frente a sin ruleset", n(p))
}
fn es_hit(k: usize, a: &str) -> String {
    format!("✓ peldaño {k}: {a}")
}
fn ko_words(a: usize, b: usize) -> String {
    format!("{a}단어 → {b}단어")
}
fn ko_cut(p: f64) -> String {
    format!("규칙 없음 대비 −{}%", n(p))
}
fn ko_hit(k: usize, a: &str) -> String {
    format!("✓ {k}단계: {a}")
}
fn zh_cn_words(a: usize, b: usize) -> String {
    format!("{a} 个词 → {b} 个词")
}
fn zh_cn_cut(p: f64) -> String {
    format!("比无规则集少 {}%", n(p))
}
fn zh_cn_hit(k: usize, a: &str) -> String {
    format!("✓ 第 {k} 级：{a}")
}
fn ja_words(a: usize, b: usize) -> String {
    format!("{a} 語 → {b} 語")
}
fn ja_cut(p: f64) -> String {
    format!("ルールなしより −{}%", n(p))
}
fn ja_hit(k: usize, a: &str) -> String {
    format!("✓ {k} 段目：{a}")
}

const EN: Lang = Lang {
    code: "en",
    pill: "Claude Code plugin · 13 agents · 0 deps",
    tag1: "Fewer tokens in. Fewer tokens out.",
    tag2: "Same answers.",
    words: en_words,
    same: "✓ same answer",
    stats_title: "Chisle vs the same model with no ruleset",
    tiles: [
        ["billed output tokens", "69% of the bare model"],
        ["answer length", "51% of the bare model"],
        ["oversized tool output", "per trimmed tool output"],
        ["accuracy", "bare model: 156/156"],
    ],
    race_title: "Same prompt. Four answers.",
    race_note: "visible answer tokens · lower is leaner",
    bare: "no ruleset",
    tokens: " tokens",
    cut: en_cut,
    race_foot: "Claude Code + Haiku 4.5 · all four in one run · the verbatim answers are below",
    reads: "what Claude reads",
    kept: "  ← error kept",
    foot1: "Raw output is billed again on every later turn",
    foot2: "Oversized tool output: ~48% smaller on average",
    ladder_title: "Before writing code, stop at the first rung that holds",
    rungs: [
        [
            "Does this need to exist at all?",
            "skip it, say so in one line",
        ],
        ["Already in this codebase?", "reuse it"],
        ["Does the stdlib do it?", "use the stdlib"],
        [
            "Native platform feature?",
            "CSS over JS, DB constraint over code",
        ],
        [
            "Already-installed dependency?",
            "use it, never add a new one",
        ],
        ["Can it be one line?", "one line"],
        ["Only then", "the minimum code that works"],
    ],
    request: "Request:",
    reqs: [
        "deep-copy a config object",
        "add a date picker",
        "format a date for the UI",
    ],
    answers: [
        "structuredClone(config)",
        "<input type=\"date\">",
        "reuse utils/formatDate()",
    ],
    hit: en_hit,
    sheet: "cheat sheet",
    notes: [
        "wire every agent on this machine",
        "preview first",
        "put everything back",
        "pause it",
        "over-engineering, one finding per line",
        "bloated code and prose, biggest cut first",
        "tool schemas you pay for but never use",
        "trim MCP output in any agent",
    ],
    filt_title: "Knows the command. Drops the noise.",
    filt_note: "raw output → what Claude reads",
    filt_sub: "Command-aware filters, built in: no extra binary, any OS. Errors, failures and summaries always stay.",
    filt_notes: [
        "256 passing tests hidden, summary kept",
        "400 passes hidden, the failure kept whole",
        "long form rewritten in git's short form",
        "one line per commit, long messages counted",
    ],
    filt_foot: "37 commands: Claude reads 28% of the raw output with Chisle, 84% with RTK. Failed runs filtered in bypass mode; | cat shows raw.",
    chars: "chars",
    strip_title: "Chisle is free. Its benchmarks aren't.",
    strip_sub: "Saved you tokens? A coffee pays for the next benchmark run.",
    free: "Chisle is free.",
    not_free: "Its benchmarks aren't.",
    body1: "Every benchmark run costs real API money.",
    body2: "Saved you tokens? A coffee pays for the next run.",
    button: "☕ Buy me a coffee",
};

const ES: Lang = Lang {
    code: "es",
    pill: "Plugin de Claude Code · 13 agentes · 0 deps",
    tag1: "Menos tokens de entrada y de salida.",
    tag2: "Las mismas respuestas.",
    words: es_words,
    same: "✓ misma respuesta",
    stats_title: "Chisle frente al mismo modelo sin ruleset",
    tiles: [
        ["tokens facturados", "69% del modelo sin reglas"],
        ["longitud de respuesta", "51% del modelo sin reglas"],
        ["salida de herramientas", "por salida recortada"],
        ["precisión", "sin reglas: 156/156"],
    ],
    race_title: "Mismo prompt. Cuatro respuestas.",
    race_note: "tokens de respuesta visibles · menos es mejor",
    bare: "sin ruleset",
    tokens: " tokens",
    cut: es_cut,
    race_foot: "Claude Code + Haiku 4.5 · los cuatro en una ejecución · las respuestas literales, abajo",
    reads: "lo que lee Claude",
    kept: "  ← error conservado",
    foot1: "La salida en bruto se vuelve a cobrar en cada turno",
    foot2: "Salida enorme de herramientas: ~48% más pequeña de media",
    ladder_title: "Antes de escribir código, detente en el primer peldaño que se cumpla",
    rungs: [
        ["¿Hace falta que exista?", "sáltalo y dilo en una línea"],
        ["¿Ya está en este código?", "reutilízalo"],
        [
            "¿Lo hace la biblioteca estándar?",
            "usa la biblioteca estándar",
        ],
        [
            "¿Una función nativa de la plataforma?",
            "CSS antes que JS, restricción de BD antes que código",
        ],
        ["¿Una dependencia ya instalada?", "úsala, nunca añadas otra"],
        ["¿Cabe en una línea?", "una línea"],
        ["Solo entonces", "el código mínimo que funciona"],
    ],
    request: "Petición:",
    reqs: [
        "copia profunda de un objeto de configuración",
        "añadir un selector de fecha",
        "formatear una fecha para la interfaz",
    ],
    answers: [
        "structuredClone(config)",
        "<input type=\"date\">",
        "reutiliza utils/formatDate()",
    ],
    hit: es_hit,
    sheet: "chuleta",
    notes: [
        "configura cada agente de esta máquina",
        "primero, una vista previa",
        "lo deja todo como estaba",
        "lo pausa",
        "sobreingeniería, un hallazgo por línea",
        "código y prosa inflados, el mayor recorte primero",
        "esquemas de herramientas que pagas y no usas",
        "recorta la salida MCP en cualquier agente",
    ],
    filt_title: "Conoce el comando. Quita el ruido.",
    filt_note: "salida en bruto → lo que lee Claude",
    filt_sub: "Filtros por comando, integrados: sin binario extra, en cualquier SO. Errores, fallos y resúmenes siempre se quedan.",
    filt_notes: [
        "256 tests aprobados ocultos, resumen intacto",
        "400 aprobados ocultos, el fallo entero",
        "forma larga reescrita en la forma corta de git",
        "una línea por commit, mensajes largos contados",
    ],
    filt_foot: "37 comandos: Claude lee el 28% de la salida en bruto con Chisle, el 84% con RTK. Los fallos se filtran en modo bypass; | cat muestra el original.",
    chars: "caracteres",
    strip_title: "Chisle es gratis. Sus benchmarks, no.",
    strip_sub: "¿Te ahorró tokens? Un café paga la próxima ejecución del benchmark.",
    free: "Chisle es gratis.",
    not_free: "Sus benchmarks, no.",
    body1: "Cada ejecución de benchmark cuesta dinero real de API.",
    body2: "¿Te ahorró tokens? Un café paga la próxima.",
    button: "☕ Invítame a un café",
};

const KO: Lang = Lang {
    code: "ko",
    pill: "Claude Code 플러그인 · 에이전트 13개 · 의존성 0",
    tag1: "입력 토큰도, 출력 토큰도 적게.",
    tag2: "답은 그대로.",
    words: ko_words,
    same: "✓ 같은 답",
    stats_title: "규칙 세트가 없는 같은 모델과 비교한 Chisle",
    tiles: [
        ["과금 출력 토큰", "규칙 없는 모델의 69%"],
        ["답변 길이", "규칙 없는 모델의 51%"],
        ["큰 도구 출력", "축소된 출력 기준"],
        ["정확도", "규칙 없음: 156/156"],
    ],
    race_title: "같은 프롬프트, 네 가지 답.",
    race_note: "보이는 답변 토큰 · 낮을수록 간결",
    bare: "규칙 없음",
    tokens: " 토큰",
    cut: ko_cut,
    race_foot: "Claude Code + Haiku 4.5 · 넷 다 한 번의 실행 · 원문 답변은 아래에",
    reads: "Claude가 읽는 내용",
    kept: "  ← 오류는 유지",
    foot1: "원본 출력은 이후 매 턴마다 다시 과금됩니다",
    foot2: "큰 도구 출력: 평균 ~48% 감소",
    ladder_title: "코드를 쓰기 전에, 성립하는 첫 단계에서 멈춥니다",
    rungs: [
        ["꼭 있어야 하나?", "건너뛰고 한 줄로 알리기"],
        ["이미 코드베이스에 있나?", "재사용하기"],
        ["표준 라이브러리로 되나?", "표준 라이브러리 쓰기"],
        [
            "플랫폼 기본 기능이 있나?",
            "JS보다 CSS, 앱 코드보다 DB 제약",
        ],
        [
            "이미 설치된 의존성이 있나?",
            "그걸 쓰고, 새로 추가하지 않기",
        ],
        ["한 줄로 되나?", "한 줄로"],
        ["그때서야", "동작하는 최소한의 코드"],
    ],
    request: "요청:",
    reqs: [
        "설정 객체 깊은 복사",
        "날짜 선택기 추가",
        "UI용 날짜 서식 지정",
    ],
    answers: [
        "structuredClone(config)",
        "<input type=\"date\">",
        "utils/formatDate() 재사용",
    ],
    hit: ko_hit,
    sheet: "치트 시트",
    notes: [
        "이 컴퓨터의 모든 에이전트 설정",
        "먼저 미리 보기",
        "전부 되돌리기",
        "일시 중지",
        "과잉 설계, 한 줄에 하나씩",
        "부풀려진 코드와 글, 큰 절감부터",
        "비용만 내고 안 쓰는 도구 스키마",
        "모든 에이전트에서 MCP 출력 축소",
    ],
    filt_title: "명령을 알고, 잡음을 걷어낸다.",
    filt_note: "원본 출력 → Claude가 읽는 것",
    filt_sub: "명령 인식 필터 내장: 추가 바이너리 없이 모든 OS에서. 오류, 실패, 요약은 항상 남긴다.",
    filt_notes: [
        "통과한 테스트 256개 숨김, 요약은 유지",
        "통과 400개 숨김, 실패는 전부 유지",
        "긴 형식을 git의 짧은 형식으로",
        "커밋당 한 줄, 긴 메시지는 줄 수만",
    ],
    filt_foot: "명령 37개: Claude가 읽는 원본 출력은 Chisle 28%, RTK 84%. 실패한 실행도 bypass 모드에서 필터링; | cat 이면 원본.",
    chars: "자",
    strip_title: "Chisle은 무료입니다. 벤치마크는 아닙니다.",
    strip_sub: "토큰을 아꼈나요? 커피 한 잔이 다음 벤치마크 비용이 됩니다.",
    free: "Chisle은 무료입니다.",
    not_free: "벤치마크는 아닙니다.",
    body1: "벤치마크를 돌릴 때마다 실제 API 비용이 듭니다.",
    body2: "토큰을 아꼈나요? 커피 한 잔이 다음 실행 비용이 됩니다.",
    button: "☕ 커피 한 잔 사 주기",
};

const ZH_CN: Lang = Lang {
    code: "zh-CN",
    pill: "Claude Code 插件 · 13 个智能体 · 零依赖",
    tag1: "输入更少 token，输出更少 token。",
    tag2: "答案不变。",
    words: zh_cn_words,
    same: "✓ 答案相同",
    stats_title: "Chisle 对比无规则集的同一模型",
    tiles: [
        ["计费输出 token", "无规则集模型的 69%"],
        ["回答长度", "无规则集模型的 51%"],
        ["超大工具输出", "按每个被精简的输出"],
        ["准确率", "无规则集：156/156"],
    ],
    race_title: "同一个提示词，四个回答。",
    race_note: "可见回答 token · 越低越精简",
    bare: "无规则集",
    tokens: " token",
    cut: zh_cn_cut,
    race_foot: "Claude Code + Haiku 4.5 · 四者同一次运行 · 原始回答见下方",
    reads: "Claude 读到的内容",
    kept: "  ← 保留错误",
    foot1: "原始输出会在之后每一轮被重新计费",
    foot2: "超大工具输出：平均缩小约 48%",
    ladder_title: "写代码之前，停在第一个成立的台阶",
    rungs: [
        ["这真的需要存在吗？", "跳过，并用一句话说明"],
        ["代码库里已经有了吗？", "直接复用"],
        ["标准库能做吗？", "用标准库"],
        [
            "平台原生功能能做吗？",
            "能用 CSS 就不用 JS，能用数据库约束就不写代码",
        ],
        ["已安装的依赖能做吗？", "用它，绝不新增依赖"],
        ["能写成一行吗？", "写成一行"],
        ["最后才", "写能工作的最少代码"],
    ],
    request: "请求：",
    reqs: ["深拷贝一个配置对象", "添加日期选择器", "为界面格式化日期"],
    answers: [
        "structuredClone(config)",
        "<input type=\"date\">",
        "复用 utils/formatDate()",
    ],
    hit: zh_cn_hit,
    sheet: "速查表",
    notes: [
        "配置本机所有智能体",
        "先预览",
        "全部还原",
        "暂停",
        "过度设计，每行一条",
        "臃肿的代码和文字，收益大的优先",
        "付了费却从不使用的工具 schema",
        "在任何智能体中精简 MCP 输出",
    ],
    filt_title: "认得命令，滤掉噪音。",
    filt_note: "原始输出 → Claude 读到的",
    filt_sub: "内置按命令过滤：无需额外二进制，任何系统可用。错误、失败和汇总始终保留。",
    filt_notes: [
        "隐藏 256 个通过的测试，保留汇总",
        "隐藏 400 个通过，失败完整保留",
        "长格式改写为 git 短格式",
        "每个提交一行，长消息只计行数",
    ],
    filt_foot: "37 条命令：用 Chisle 时 Claude 读原始输出的 28%，用 RTK 时 84%。失败的运行在 bypass 模式下也会过滤；加 | cat 看原始输出。",
    chars: "字符",
    strip_title: "Chisle 是免费的，基准测试可不是。",
    strip_sub: "帮你省下了 token？一杯咖啡就能支付下一次基准测试。",
    free: "Chisle 是免费的。",
    not_free: "基准测试可不是。",
    body1: "每次运行基准测试都要花真实的 API 费用。",
    body2: "帮你省下了 token？一杯咖啡就能支付下一次运行。",
    button: "☕ 请我喝杯咖啡",
};

const JA: Lang = Lang {
    code: "ja",
    pill: "Claude Code プラグイン · 13 エージェント · 依存ゼロ",
    tag1: "入力も出力も、トークンは少なく。",
    tag2: "答えは同じ。",
    words: ja_words,
    same: "✓ 同じ答え",
    stats_title: "ルールなしの同じモデルと比べた Chisle",
    tiles: [
        ["課金出力トークン", "ルールなしモデルの 69%"],
        ["回答の長さ", "ルールなしモデルの 51%"],
        ["巨大なツール出力", "削減した出力あたり"],
        ["精度", "ルールなし：156/156"],
    ],
    race_title: "同じプロンプト、4 つの回答。",
    race_note: "表示される回答トークン · 低いほど簡潔",
    bare: "ルールなし",
    tokens: " トークン",
    cut: ja_cut,
    race_foot: "Claude Code + Haiku 4.5 · 4 つとも同じ実行 · 回答の原文は下に",
    reads: "Claude が読む内容",
    kept: "  ← エラーは残す",
    foot1: "生の出力は以降のターンごとに再課金されます",
    foot2: "巨大なツール出力：平均で約 48% 縮小",
    ladder_title: "コードを書く前に、最初に当てはまる段で止まる",
    rungs: [
        ["そもそも必要か？", "飛ばして、1 行でそう伝える"],
        ["すでにコードベースにあるか？", "再利用する"],
        ["標準ライブラリでできるか？", "標準ライブラリを使う"],
        [
            "プラットフォームの標準機能は？",
            "JS より CSS、アプリより DB 制約",
        ],
        [
            "インストール済みの依存関係は？",
            "それを使い、新しく足さない",
        ],
        ["1 行で書けるか？", "1 行で書く"],
        ["そこで初めて", "動く最小限のコード"],
    ],
    request: "リクエスト：",
    reqs: [
        "設定オブジェクトをディープコピー",
        "日付ピッカーを追加",
        "UI 用に日付を整形",
    ],
    answers: [
        "structuredClone(config)",
        "<input type=\"date\">",
        "utils/formatDate() を再利用",
    ],
    hit: ja_hit,
    sheet: "チートシート",
    notes: [
        "このマシンの全エージェントを設定",
        "まずプレビュー",
        "すべて元に戻す",
        "一時停止",
        "過剰設計を 1 行ずつ指摘",
        "膨らんだコードと文章、効果の大きい順",
        "払っているのに使わないツールスキーマ",
        "どのエージェントでも MCP 出力を削減",
    ],
    filt_title: "コマンドを見て、ノイズを落とす。",
    filt_note: "生の出力 → Claude が読む内容",
    filt_sub: "コマンド別フィルターを内蔵：追加バイナリ不要、どの OS でも。エラー・失敗・要約は必ず残す。",
    filt_notes: [
        "成功したテスト 256 件を非表示、要約は残す",
        "成功 400 件を非表示、失敗はそのまま",
        "長い形式を git の短い形式に",
        "コミットごとに 1 行、長いメッセージは行数のみ",
    ],
    filt_foot: "37 コマンド：Claude が読む生の出力は Chisle で 28%、RTK で 84%。失敗した実行も bypass モードでフィルター；| cat で生の出力。",
    chars: "文字",
    strip_title: "Chisle は無料。ベンチマークは無料じゃない。",
    strip_sub: "トークンを節約できた？コーヒー 1 杯で次のベンチマークが回せます。",
    free: "Chisle は無料。",
    not_free: "ベンチマークは無料じゃない。",
    body1: "ベンチマークを回すたびに実際の API 料金がかかります。",
    body2: "トークンを節約できた？コーヒー 1 杯で次の実行が回せます。",
    button: "☕ コーヒーをおごる",
};
