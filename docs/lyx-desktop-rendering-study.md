# LyX 桌面公式编辑与绘制机制研究

研究对象是本机 `D:\tool-base\lyx`，提交 `556cfcc182`（2026-08-18）。本文只讨论桌面编辑区的交互与显示链路，不讨论 LaTeX 最终导出速度。

## 结论

LyX 快的首要原因不是它更快地生成公式图片，而是正常编辑根本不生成图片。公式在内存里是长期存在的结构树，字符、符号、分数线、根号、上下标和矩阵由 Qt 字体与基础绘图指令直接绘制。一次按键只修改光标附近的树结点，并请求当前段落或当前行重排。

LyX 的高保真公式预览是第二条、可选的显示路径。启用即时预览时，未进入编辑状态的公式可以显示异步生成的位图；光标进入公式后立即切回原生结构绘制，离开公式后才刷新位图。位图生成不在按键的同步路径内，而且多个待生成片段会合并为一次外部 LaTeX 任务。

这套设计可以概括为：

```text
文件载入/粘贴/确认命令
        │
        ▼
常驻公式对象树 MathData / InsetMath
        │
        ├── 编辑中：metrics + Qt 字形/线条直接绘制
        │              只重排当前段落、只重画脏行
        │
        └── 非编辑：可选的异步高保真位图
                       片段键缓存、批量生成
```

## 1. 公式是编辑模型，不是渲染产物

`MathData` 是 `MathAtom` 的容器，每个原子持有一个 `InsetMath`，嵌套 inset 组成公式树。文件中的 `Formula` 在载入时由 `mathed_parse_normal` 转成 `InsetMathHull`；普通字符输入则直接向当前 `MathData` 插入新的 `InsetMathChar`。因此，LyX 不会在每次输入后重新扫描文档并重新解析所有公式。

该模型还区分编辑结构与显示结构。`MathRow` 会在计算排版时把可展开宏线性化，以得到正确的数学间距；原始宏仍然是编辑时的一个对象。宏实例内部长期保存 `expanded_`，只有 `needsUpdate_` 为真时才重新展开。

相关实现：

- `D:\tool-base\lyx\src\mathed\MathData.h`
- `D:\tool-base\lyx\src\mathed\InsetMath.h`
- `D:\tool-base\lyx\src\mathed\InsetMathHull.cpp:2418`
- `D:\tool-base\lyx\src\mathed\MathRow.cpp:152`
- `D:\tool-base\lyx\src\mathed\InsetMathMacro.cpp:339`
- `D:\tool-base\lyx\src\mathed\InsetMathMacro.cpp:689`

## 2. 编辑公式由原生 painter 直接画

公式布局分成 `metrics` 和 `draw` 两步。`metrics` 递归计算每个结点的宽度、上升部和下降部；`draw` 使用已计算的尺寸放置子结点。

字符和符号使用 Qt 字体度量及文本绘制。分数递归计算分子、分母尺寸后画一条横线；上下标切换到 script 字号后放置子树；根号、括号和装饰符使用字形或线段。整个过程没有 SVG DOM、图片解码、跨进程调用或完整 Typst 编译。

`MathData::metrics` 每次调用都构造一个新的 `MathRow` 并递归计算尺寸，`MathData::draw` 再按同样的规则放置子结点。被缓存的是坐标：绘制把结果写进当前 `BufferView` 的 `CoordCache`，键是对象指针与尺寸，所以同一帧内的后续查询不需要字符串哈希，也不需要重新定位源码范围。度量本身按需重算，不能把 `MathData::metrics` 说成命中缓存；`MathData::metrics` 开头还会调用 `updateMacros`，这也是按需执行而不是缓存。

相关实现：

- `D:\tool-base\lyx\src\mathed\MathData.cpp:344`
- `D:\tool-base\lyx\src\mathed\MathData.cpp:356`
- `D:\tool-base\lyx\src\mathed\MathData.cpp:424`
- `D:\tool-base\lyx\src\mathed\MathRow.cpp:247`
- `D:\tool-base\lyx\src\mathed\MathRow.cpp:328`
- `D:\tool-base\lyx\src\mathed\InsetMathChar.cpp:107`
- `D:\tool-base\lyx\src\mathed\InsetMathFrac.cpp:180`
- `D:\tool-base\lyx\src\mathed\InsetMathScript.cpp:289`
- `D:\tool-base\lyx\src\CoordCache.h`

字体度量本身也有缓存。`GuiFontMetrics` 对字符串宽度、字符串尺寸、断行结果以及部分平台上的 `QTextLayout` 设置了有界缓存。这让重复公式字符的测量接近普通文本测量成本。

- `D:\tool-base\lyx\src\frontends\qt\GuiFontMetrics.cpp:77`
- `D:\tool-base\lyx\src\frontends\qt\GuiFontMetrics.cpp:252`

字体来源同样值得记一笔，因为它决定了"LyX 同款字体"到底是什么。LyX 的 Qt 界面把 `lib/fonts/` 下的 12 个 BaKoMa Computer Modern TTF 注册进 `QFontDatabase`（`cmex10 cmmi10 cmr10 cmsy10 dsrom10 esint10 eufm10 msam10 msbm10 rsfs10 stmary10 wasy10`），符号家族（CMR/CMSY/CMM/CMEX/MSA/MSB/EUFRAK/RSFS/STMARY/WASY/ESINT/DS）按名称映射到这些家族。但数学字母并不用 `cmmi10`：`mathnormal` 是"正文家族 + 斜体形状"（`MathSupport.cpp` 的 `fontinfos` 表），也就是用用户配置的正文衬线字体排斜体，只有命名符号才落到 CM 符号家族。这些 BaKoMa 字体是 **TeX 编码**：`cmmi10` 的小写希腊在槽位 `0x0B–0x21`、`cmsy10` 的 ≤ 在 `0x14`、`cmex10` 的 ∑text 在 `0x50`（字符 `X`），字体 cmap 里没有 U+03B1/U+2264/U+2211。LyX 能这么做是因为它自带字符编码表（`lib/symbols` 给出家族与槽位，如 `leq cmsy 20 163 mathrel &#x2264;`）并按自己的编码取字形；Qt 的 `drawText` 只能按 Unicode 取字形，所以这套字体不能被直接复用。本项目因此用随附的 New Computer Modern Math——Typst 自己的 CM 复刻，也是编译结果所用的字体，Unicode 覆盖齐全。

- `D:\tool-base\lyx\src\frontends\qt\GuiFontLoader.cpp:37`
- `D:\tool-base\lyx\src\frontends\qt\GuiFontLoader.cpp:77`
- `D:\tool-base\lyx\src\mathed\MathSupport.cpp:873`
- `D:\tool-base\lyx\lib\fonts\README`
- `D:\tool-base\lyx\lib\symbols:319`

## 3. 更新粒度以段落、可见区和脏行为边界

常见输入命令带 `Update::SinglePar`（`UpdateFlags.h` 中的单段更新标记）标记。LyX 先尝试只重新断行当前段落；如果段落高度没有变化，并且宽度变化不影响外层布局，就保留其他段落的 metrics。只有该快速路径失败时才扩大重排范围。

即使需要更新文档布局，`TextMetrics::updateMetrics` 也只保证锚点上下覆盖可见窗口的段落有 metrics，并丢弃屏幕外的段落缓存。绘制时再次裁剪不可见行。

每个 `Row` 有 `changed` 位。部分重绘时，未变行不重画文字；必要的 inset 和装饰仍可单独绘制。Qt 工作区可以把结果保存在一张屏幕大小的 `QImage` 中，只把曝光或变化的矩形复制到窗口。

相关实现：

- `D:\tool-base\lyx\src\BufferView.cpp:554`
- `D:\tool-base\lyx\src\BufferView.cpp:3342`
- `D:\tool-base\lyx\src\BufferView.cpp:3403`
- `D:\tool-base\lyx\src\BufferView.cpp:3901`
- `D:\tool-base\lyx\src\TextMetrics.cpp:225`
- `D:\tool-base\lyx\src\TextMetrics.cpp:492`
- `D:\tool-base\lyx\src\TextMetrics.cpp:1946`
- `D:\tool-base\lyx\src\frontends\qt\GuiWorkArea.cpp:1339`
- `D:\tool-base\lyx\src\frontends\qt\GuiWorkArea.cpp:1372`

## 4. 高保真图片不阻塞编辑

`InsetMathHull::previewState` 明确要求公式当前没有被编辑，并且缓存图像已经可用，才选择预览图。否则 `metrics` 和 `draw` 都使用原生公式树。

光标离开公式时，`notifyCursorLeaves` 才调用 `reloadPreview`。光标进入公式时只切换显示状态并请求局部重排，不先等待新图。因此，即时预览失败或变慢不会拖慢公式输入。

生成预览前，LyX 只收集该公式实际引用的宏定义，并把上下文、字号和颜色写进片段。`PreviewLoader` 以完整 LaTeX 片段为键检查缓存；所有 `InQueue` 片段会写入同一个临时 LaTeX 文件，再由一个异步进程批量转换成各自的位图。图像完成后通过信号只通知对应 inset 更新。

相关实现：

- `D:\tool-base\lyx\src\mathed\InsetMathHull.cpp:512`
- `D:\tool-base\lyx\src\mathed\InsetMathHull.cpp:535`
- `D:\tool-base\lyx\src\mathed\InsetMathHull.cpp:657`
- `D:\tool-base\lyx\src\mathed\InsetMathHull.cpp:868`
- `D:\tool-base\lyx\src\mathed\InsetMathHull.cpp:933`
- `D:\tool-base\lyx\src\graphics\PreviewLoader.h`
- `D:\tool-base\lyx\src\graphics\PreviewLoader.cpp:517`
- `D:\tool-base\lyx\src\graphics\PreviewLoader.cpp:574`
- `D:\tool-base\lyx\src\insets\RenderPreview.cpp`

## 5. 与当前 TypFormula 桌面端的差异

当前实现已经采用 Typst `Source::edit`，并且只对 Typst 报告的重解析区间重建公式投影。这解决了“每次重新解析所有公式”的主要问题。不过，显示层仍有几处比 LyX 粗：

1. `scan` 每次仍递归遍历整棵文档语法树，以重新收集公式范围和样式。
2. `Editor.install_objects` 每次遍历所有公式对象；窗口的 `project` 也遍历所有公式准备 attachment 信息。
3. `FormulaObject.intrinsicSize` 和 `drawObject` 都调用 `Typesetter.layout`。同一公式可能在一次 Qt 布局/绘制周期内重复构造 `Box` 树。
4. Raw 已经有按源码共享的位图缓存，但可见性检查仍遍历所有公式及编辑器对象。
5. 每次输入后的后台任务仍会启动 attachment 查询和语义高亮。它们虽有 550 ms 防抖，仍可能与继续输入争用 Rust 服务、Python 主线程或 Qt 更新。（宏预热这一项已随之取消：宏内片段改走普通取图路径。）
6. `QTextDocument` 管理正文很方便，但对嵌入对象的脏区、尺寸缓存和可见区虚拟化控制不如 LyX 自己的 `BufferView + TextMetrics` 精确。

对应位置：

- `src/desktop.rs:17`
- `desktop/editor.py:100`
- `desktop/editor.py:113`
- `desktop/mathview.py:209`
- `desktop/mathview.py:217`
- `desktop/window.py:158`
- `desktop/window.py:378`
- `desktop/window.py:437`

## 6. 建议采用的 LyX 式改造

### 第一阶段：缓存显示树与 Box，保留现有 Qt 文本控件

为每个公式建立长期存在的 `FormulaRecord`，至少保存：

- 稳定公式 ID；
- Typst 源码区间和该公式版本号；
- 结构化 `MathData/view`；
- 按 `(公式版本, 字体, 缩放, 显示模式)` 缓存的 `Box`；
- Raw 引用、宏依赖和 attachment 依赖；
- 当前是否可见、是否正在编辑、是否需要重新度量或重画。

`intrinsicSize` 与 `drawObject` 必须取得同一个缓存 Box。普通光标移动只重画插入符和边框；公式内字符变更只使当前公式的 view 与 Box 失效；正文字符变更不触碰任何公式；字体缩放才使所有 Box metrics 失效。

### 第二阶段：把后台任务从“文档变化”改成“依赖变化”

attachment 只在表达式或其可见作用域依赖改变时运行。Raw 仅在首次进入可见区、Raw 自身源码变化，或影响它的 `let` 发生变化时请求引擎；宏定义里的 Raw 也走同一条路，只是它的源码区间落在定义文本上。

语义高亮可以独立防抖并允许取消，不能阻塞公式对象更新。所有后台结果继续用文档 revision 校验，过期结果直接丢弃。

### 第三阶段：公式投影与可见区虚拟化

载入文件时只建立轻量的公式索引。对可见窗口及上下少量预取区域构造结构显示树；滚入时懒构造，滚出后保留有界 LRU。`scan` 应接收 Typst 的变化区间并更新区间索引，而不是每次完整遍历语法树。

这一步会让大文档的首次打开成本由“全部公式复杂度”下降为“语法索引 + 首屏公式复杂度”。

### 第四阶段：需要进一步逼近 LyX 时，替换正文画布

若前三步以后 `QTextDocument` 的嵌入对象布局仍是瓶颈，可以把编辑区改成自绘 `QAbstractScrollArea`：维护段落 metrics、行盒、公式盒、源位置映射和脏行集合，只绘制可见行。Tinymist 继续提供补全与语义能力，Typst engine 继续负责 PDF 和 Raw 的高保真结果。

这是最接近 LyX 的方案，但工作量明显更大，因为普通文本的选区、IME、双向文字、无障碍和复制粘贴都要由应用承担。应在前三阶段有基准测试证明 `QTextDocument` 确实是主要瓶颈后再做。

## 7. 不应照搬的部分

LyX 的原生公式 painter 是对 TeX 数学模型的长期实现，不能简单替代 Typst 的完整排版语义。TypFormula 应继续采用混合策略：已支持的结构结点用原生 painter 保证编辑速度；引擎排版结果用于**叶子**显示来源（Raw、附件位置），最终 PDF 始终交给 Typst engine。

需要更正一点：LyX 不会把无法结构化的内容变成位图。`InsetMathUnknown` 保留源码文本并用原生字体直接画出来（未完成时画成红色），`metricsStrRedBlack` 直接给出尺寸，因此它始终可继续编辑。

- `D:\tool-base\lyx\src\mathed\InsetMathUnknown.cpp:48`
- `D:\tool-base\lyx\src\mathed\InsetMathUnknown.cpp:57`

所以“无法安全结构化就退回引擎位图”既不是 LyX 的做法，也不适合作为默认策略：能保留源码、可继续原位编辑的原生表示优先，位图只用于那些只有引擎才说得清的排版结果。

LyX 的 `BufferView::processUpdateFlags` 目前仍会调用全局 `buffer_.updateMacros()`，源码注释也承认这很昂贵。我们应借鉴它的常驻展开结果和失效标记，而不是照搬全局宏更新。

## 建议的验收指标

- 在公式外连续输入时：重建公式数为 0，Raw 请求数为 0，attachment 请求数为 0。
- 在一个结构公式内输入时：只重建并重新度量当前公式。
- 光标移动和选择变化：不重建 view，不重新度量 Box，只请求局部重画。
- 相同 Raw 在任意公式中：一个缓存项、一次引擎请求。
- 1000 个公式的文档首次显示：只投影首屏和预取区公式。
- 单次按键到屏幕更新的 P95 小于 16 ms；后台服务不让主线程出现超过 8 ms 的连续工作。
