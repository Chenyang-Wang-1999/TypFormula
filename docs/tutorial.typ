#import "@preview/latex-lookalike:0.1.4": *

#let annote-color=rgb("dfeefa")
#let annote(x)=rect(fill:annote-color, width: 100%, x)

#let todo(x)=rect(fill:rgb("ffe7d1"), width: 100%, {[TODO: #x]})

#set heading(numbering: "1.")
#set document(title: "TypFormula 使用教程")
#title()

`TypFormula` 是一个对公式做了可视化支持的 Typst 编辑器。

除了对公式的渲染功能以外，它和一个普通的 Typst 编辑器没有任何差别。

#todo()[已知问题：当命令输入一半时，遇到括号、引号等没闭合的情况而破坏了已有的宏，程序可能会直接崩溃。一个临时性的规避方法是，输一个 let 然后删掉，在宏编辑框里编辑。]

#todo()[已知问题：前端编辑器的换行不太对劲。]

下面，详细介绍数学模块的用法。

= 创建一个公式框

使用“行内公式”或“行间公式”按键可以创建公式框。你也可以在"工具-设置-快捷键"中设置它们的快捷键。

$pi = 3.1415926 dots$

$ pi = 4 sum_(n = 0)^(oo) frac((- 1)^(n), 2 n + 1) = 1 - frac(1, 3) + frac(1, 5) - frac(1, 7) + dots.c $

你也可以在源代码模式输入一个公式  `$pi approx 3.14$`，然后在公式末尾回车，编辑器自动转为公式框。$pi approx 3.14$

宏定义中的公式不能转为公式框：

#[
#let mathbf(x) = $bold(upright(#x))$
$ mathbf(F) = m mathbf(a) $
]

= 基本用法

公式框有三种编辑模式：
- 普通模式：进入公式框时，默认处于普通模式。在普通模式下，输入的字符之间默认都带一个空格。例如：依次键入 `f,r,a,c`，得到： $f r a c$  (源码：`$f r a c$`)。
  #annote()[两个例外：
  - 数字：由于 Typst 对数字的特殊处理，普通模式下不会在数字之间加空格。但是，仅限于数字。在数字中间输入 `"."` 还是默认添加空格的。如果你想输入紧凑的小数，请使用命令模式。
  - 除号：输入 `"/"` 时，公式编辑器会自动创建分式，并转成`frac`。
  ]

- 命令模式：在公式框中按下 `"\"` 即可进入命令模式。命令模式仍然是 Typst 语法。这里的"\"只是启动命令模式的开关，不会进入源码。按回车后，编辑器自动解析。
例如，可以在公式框中按`"\"`进入命令模式，输入`frac(1,2)`，回车，就会得到：$ frac(1, 2) $ 
  您还可以用方向键将光标移动到分子分母对应位置，并编辑它们。例如，将分子改成$pi$:
  #align(center, image("Figures/pi-over-2.png", width:50%))
  #annote()[如果命令编译不过，自动回退源码框。例如 #align(center, image("Figures/render-failed.png", height: 20pt))]


- 文本模式：在公式中按下双引号，即可进入文本模式：$ "Hello, TypFormula!" $ 
  文本会通过一个黄色的框子高亮。
  #align(center, image("Figures/text-mode.png", height:20pt))

= 内置渲染 vs. SVG 渲染 <sec:svg-render>

相较于其他可视化编辑器，比如 LyX、TyX、MathML 与 AxMath，TypFormula 采用了一种高度可扩展的渲染方式。
得益于 Typst 强大的实时编译能力，我们不需要一一复刻 Typst 引擎的所有规则。
当编辑器遇到没有记录的命令时，它会调用 Typst 引擎实时编译，并在编辑器内渲染成 svg 块。事实上，我们只内置了 40 种符号和 10 余种宏（见`config`目录），剩下的部分都依赖引擎自己渲染。

例如，由于 unicode 只支持一种花体字，我们没有将 `scr` 和 `cal` 做成字体。但编辑器仍可以正常显示：
$ cal(A) (k) : = chevron.l psi (k) | upright(i) partial_(k) | psi(k) chevron.r $

你也可以引入外部库，例如：

#[
#import "@preview/physica:0.9.8": *
$ tensor(F, + mu, - nu) = partial^(mu) A_(nu) - partial_(nu) A^(mu) $

$ "clk:" & signals("|1....|0....|1....|0....|1....|0....|1....|0..", step: #0.5em) \
"bus:" & signals(" #.... X=... ..... ..... X=... ..... ..... X#.", step: #0.5em) $
]

#annote()[- 为了兼顾效率，编辑器内的公式预览并不一定与文档一致。这是一种无奈的取舍。编辑器的缓存机制是：当某个位置命令第一次被编译成 SVG 时，就将其缓存下来。比如本文档的所有 `partial` 实际上是第一次出现的那个 `partial` 的副本。好在，我们把颜色归一化了，统一设成了黑色。]

#todo()[其实这个缓存机制理论上还能进一步优化。比如，增加一个缓存加载和缓存存储的方式，放到用户配置文件里。]

SVG 渲染有三种不同的触发条件，分别是"Raw", "RawMacro" 和 "未知Style"。这里涉及 TypFormula 的底层实现。首先，后端会调用 Typst engine 获取语法树。在解析语法树时，会遇到这样几种情况：
1. 出现了未知符号，即，不含参，且不在 `config/symbols.json` 中的命令，如 `in`, `RR`。此时，后端会将该内容识别为 "Raw"，接着前端会将它们解析成图片并缓存。一旦生成图片后，就无法再编辑源码内容。但你可以将符号整体删除。
2. 出现了未知函数，即，含参，且不在 `config/commands.json`  中的命令，如 `cal`, `bb`。此时，后端会将该内容识别为 "RawMacro"。前端会将这些命令解析成图片，但是你仍然可以进入内部编辑源码。
3. 字体风格，即 `config/commands.json` 中被标记为 "style" 的命令。这些命令通常是通过 unicode 字符替换实现渲染的，所以性能开销会远远低于图片。然而，当一个 Style 里不止包含纯字符时，前端渲染会变得非常棘手。这种情况下，回退到 RawMacro 的处理方式。例如：
$ bold(frac(1, 2)) $

关于内置渲染机制，将在下一章详细讲解。

#annote()[目前的程序暂时不支持热加载。修改 `config` 之后要运行 `build-desktop.cmd` 来重新编译后端。]

= 内置渲染机制：

TypFormula 包含以下几种内置渲染方式：
+ 内置 unicode 字符
+ 字体风格 (Style)
+ 装饰
+ 分式与根式
+ 上下标
+ 多行公式与矩阵
+ 宏
下面将依次讲解。

== 内置 unicode 字符与字体风格

在 `config/symbols.json` 中有约40个内置 unicode 字符。当解析器解析出这些字符的编码之一时，相应片段的渲染直接由这些 unicode 字符实现。这些字符只是在公式编辑器上渲染，不会修改底层源代码。所以不用担心编辑器会改乱你的代码。

字体风格(Style)是一种特殊的函数，通过配置文件标注。在 `config/commands.json` 中，有两个命令被标注为了"style"，分别是 `bold` 和 `upright`。
style 的渲染方式是将 style 名称和需要渲染的 unicode 字符传入 Typst 引擎，得到带有变体的 unicode 字符。前端以*文本*的形式显示这些字符。当返回结果为空，或者待渲染部分不是字符串时，回退到 SVG 渲染。

允许以字符形式渲染的对象有：
- 普通字符：$bold(upright(A))$
- unicode 字符：$bold(upright(alpha))$
- 文本：$bold(upright("Hello,mathbf"))$
当然，其他对象也可以通过 SVG 渲染方式得到正确的预览，但如果你仔细看，会发现以上三种对象的预览是纯文本，而其他的是图（如 #ref(<sec:svg-render>) 的 $bold(frac(1, 2))$）。

== 装饰
装饰(Decoration)是一种特殊的函数，通过配置文件标注。后端识别到装饰时，会返回一个标志字符，前端根据这个字符为待装饰内容添加标注。配置文件里的"line"和"sqrt"也是一种装饰。

如何区分装饰和RawMacro呢？可以看编辑内部元素的时候，公式编辑器的行为是装饰还是宏展开。例如：

#grid(columns: 2, rows: 2, align:  horizon, 
  [- 装饰：$overline(x x x)$ : ],
  image("Figures/overline.png", height:16pt),
  [- RawMacro: $overbrace(x x x x)$:],
  image("Figures/overbrace.png", height:16pt),

)

== 分式与根式，上下标
由于分式与根式使用频率很高，同时渲染方式特殊，所以单独处理。

分式和 LyX 的体验几乎一致，略。

根式分为 `sqrt` 和 `root`。`sqrt` 的底层实际上被识别成了一种装饰——因为它只有一个底数需要处理。真正特殊处理的是 `root`。例如
$ root(3, 3) $

上下标的处理也是 TypFormula 的一大特色。与其他数学编辑器不同，TypFormula 的上下标位置是实时从引擎中获取的。例如，行内公式：$lim_(x -> 0) sin(x) \/ x = 1$，而行间公式：
$ lim_(x -> 0) frac(sin(x), x) = 1 $

配合 SVG 编译，我们甚至可以实现以下这种可变箭头效果：

$ M stretch(->)^("Continuous map" f) C \\ { 0 } stretch(->)^(g : z mapsto frac(z, | z |)) S^(1) $

对于自定义的 operator，公式编辑器也可以正确处理上下标。

$ op("Hello, operator!", limits: #true)_("主" -> 6) = "Alibaba" $

== 多行公式与矩阵
在公式编辑器中进入命令模式，输入 `& \ &` 即可进入多行公式模式。
$ f (x , y) = & g (x) + h (y) \
= & sin ( x ) + cos ( y ) $
可以点击 “矩阵增加行(列)” 选项卡来为多行公式增加一行(列)。也可以设置自己的快捷键。快捷键配置文件默认放置在 `%AppData%/roaming/TypFormula/`中。

#todo()[为什么没有 “删除行(列)” ？单纯是因为我给忘啦！]

多行公式是我最喜欢的功能。利用多行公式+复制粘贴推公式，我甚至可以获取比草稿纸更快的速度。

矩阵也是类似。矩阵支持 "mat", "vec" 和 "cases"。
$ vec(1, 2, 3) , mat(1, 2; 3, 4) , cases(1 "if" x > 0, - 1 "if" x < 0) $

#todo()[考虑将矩阵渲染成“裸列表”+“装饰”的形式，这样可以支持多种矩阵。然后 `lr` 也一起实现。]

== 宏
正如上文中的 mathbf，TypFormula 编辑器支持将当前文档中的“let + 公式环境” 识别成宏，并且在编辑器中展开。例如：

#let pd(f, x) = $frac(partial #f, partial #x)$

$f$对$x$的偏导数是$pd(f, x)$

打开源码栏，可以看到上面的偏导数的源码仍是 `pd(f,x)`。它只是渲染成了分式的样子。当光标移入这个偏导数中，会发现它和普通的分式不一样。
首先，它不支持上下移动。从$x$移到$f$需要按左键而非上键。其次，公式中的$partial$无法删除。这就是因为它只是宏在编辑器上的渲染，而非一个真正的表达式。

宏还可以支持一个变量多次引用。例如：

#let jac(f,g,x,y) = $mat(pd(#f, #x), pd(#f, #y); pd(#g, #x), pd(#g, #y))$

$f , g$对$x , y$的雅可比矩阵是
$ jac(f, g, x, y) $

可以看到，当光标移至$f$处时，矩阵第一行两个分子同时修改。

#annote()[LyX 的宏也是同样的逻辑，但是在 LyX 中用源码模式定义的宏无法可视化展开。]
