module Noise

let hashTable x =
    let l: int array = [|
        7;
        47;
        4;
        21;
        22;
        63;
        10;
        39;
        67;
        96;
        71;
        58;
        45;
        40;
        95;
        93;
        62;
        17;
        34;
        52;
        97;
        6;
        82;
        50;
        80;
        44;
        60;
        29;
        3;
        35;
        2;
        66;
        16;
        70;
        38;
        32;
        91;
        48;
        88;
        46;
        69;
        36;
        59;
        12;
        31;
        20;
        72;
        84;
        0;
        37;
        1;
        78;
        98;
        25;
        19;
        74;
        26;
        24;
        75;
        77;
        89;
        15;
        55;
        49;
        56;
        76;
        85;
        81;
        30;
        33;
        57;
        92;
        14;
        5;
        43;
        83;
        11;
        61;
        41;
        65;
        9;
        86;
        90;
        8;
        87;
        68;
        51;
        64;
        42;
        23;
        94;
        73;
        79;
        13;
        54;
        27;
        18;
        28;
        53;
        99
    |]
    l.[x]

let myHash (key : string) =
    let p: int = 124901413
    let tableSize: int = 100
    let mutable ans: int = 0

    for c: char in key do
        ans <- (ans * p + int (c)) % tableSize

    ans <- abs ans
    
    hashTable ans

let isEven (x: int) = 0 = x % 2

let nextMap func (x: int) (y: int) =
    let point: bool * bool = (isEven x, isEven y)
    match point with
    | (true,  true ) -> func x y
    | (false, true ) -> (func x y + func (x + 1) y) / 2.0
    | (true,  false) -> (func x y + func x (y + 1)) / 2.0
    | (false, false) -> (func x y + func (x + 1) y + func x (y + 1) + func (x + 1) (y + 1)) / 4.0

let bokashi func (x: int) (y: int) =
    let mutable ans: float = 0.0
    let mutable count: int = 0

    for i: int in [-5 .. 5] do
        for l: int in [-5 .. 5] do
            if i * i + l * l <= 25
                then
                    ans <- ans + func (x + i) (y + l)
                    count <- count + 1
    
    ans / float count

let perlinMap (seed: 'a) (x: 'a) (y: 'a)  = float (myHash <| string seed + string x + string y)

type PerlinNoise = class
    val private seed : int
    new (seed: int) = {
        seed = seed
    }
    member private this.map64 (x : int) (y : int) = perlinMap this.seed x y
    member private this.map32 (x: int) (y: int) = nextMap (perlinMap this.seed) x y
    member private this.map16 (x: int) (y: int) = nextMap (nextMap (perlinMap this.seed)) x y
    member private this.map8  (x: int) (y: int) = nextMap (nextMap (nextMap (perlinMap this.seed))) x y
    member this.mainMap (x: int) (y: int) = bokashi this.map8 x y / 2.0 + bokashi this.map32 x y / 4.0 + bokashi this.map16 x y / 8.0 + bokashi this.map8 x y / 8.0
    end