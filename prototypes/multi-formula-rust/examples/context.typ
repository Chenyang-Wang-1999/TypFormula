#set page(width: 18cm, height: auto, margin: 1cm)
#set text(size: 12pt)
#show math.equation: set text(fill: rgb("#265a46"))

#let twice(x) = $#x + #x$

Inline: $frac(a, b)$ and $twice(alpha)$.

$ sqrt(x^2 + y^2) + mat(a, b; c, d) $

Raw fallback, using this document's show rules: $cancel(x + y)$.

#[
  #let twice(x) = $frac(#x, 2)$
  #show math.equation: set text(fill: rgb("#a45332"))
  Inner scope: $twice(beta) + cancel(z)$.
]

Outer scope again: $twice(gamma)$.

Empty editable slots: $ frac("", "") $.
