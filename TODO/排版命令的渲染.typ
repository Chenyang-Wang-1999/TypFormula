有些排版命令，例如 `$quad$` (`:= h(amount: 1em)`)， 并不能单独渲染成字符。

= 临时 walkaround
+ 新建“Code”类型（区别于 Raw），这一类型在前端显示源码，不编译。
+ `config` 里加一个 `codes.json`，指定哪些 commands 或 symbols 不编译

= 修复方向
+ 研究 Typst engine 是否能提供排版命令类型信息