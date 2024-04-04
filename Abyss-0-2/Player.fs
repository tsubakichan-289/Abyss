module Player

type Direction = 
    | W
    | E
    | D
    | C
    | X
    | Z
    | A
    | Q



type Player =
    val mutable direction : Direction

    new () = {
        direction = W
    }