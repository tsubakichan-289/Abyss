module Game 

open System.Windows.Forms
open System.Drawing

open Dungeon
open Chunk
open Tile
open Command
open Debug
open Player

type Mode = 
    | Neutral
    | Command 
    | Debug

type State =
    | Moving
    | Stay

type Game = class
    val mutable cam : int * int
    val mutable chunkXY : int * int
    val mutable chunks : array<array<Chunk>>
    val mutable text : string
    val mutable mode : Mode
    val mutable state : State
    val mutable moving_D : int
    val player : Player
    val dungeon : array<Dungeon>
    val dungeonIndex : int
    val seed : int

    new (_seed) = {
        mode = Neutral
        cam = 0, 0
        chunkXY = 0, 0
        chunks = [||]
        text = ""
        state = Stay
        player = new Player ()
        seed = _seed
        dungeon = [|new Dungeon (_seed)|]
        dungeonIndex = 0
        moving_D = 0
    }

    member private this.direction2d_xy =
        if this.state = Stay
            then 0, 0
            else
                match this.player.direction with
                | W -> 0              , this.moving_D
                | E -> this.moving_D  , this.moving_D
                | D -> this.moving_D  , 0
                | C -> this.moving_D  , - this.moving_D
                | X -> 0              , - this.moving_D
                | Z -> - this.moving_D, - this.moving_D
                | A -> - this.moving_D, 0
                | Q -> - this.moving_D,this.moving_D

    member this.tick = 
        match this.state with 
        | Stay -> ()
        | Moving -> 
            if this.moving_D = 16
                then
                    this.state <- Stay
                    this.moving_D <- 0
                else 
                    this.moving_D <- this.moving_D + 1
            

    member this.setChunks =
        let dungeoN: Dungeon = this.dungeon.[this.dungeonIndex]
        for i: int32 in -1 .. 1 do
            for l in -1 .. 1 do
                match this.chunkXY with
                | (a: int, b: int) ->
                if dungeoN.isIndex (a + i) (b + l)
                    then ()
                    else
                        dungeoN.addChunk (a + i) (b + l)
                        match (dungeoN.isIndex (a + i - 1) (b + l - 1)
                              ,dungeoN.isIndex (a + i)     (b + l - 1)
                              ,dungeoN.isIndex (a + i + 1) (b + l - 1)
                              ,dungeoN.isIndex (a + i - 1) (b + l)
                              ,dungeoN.isIndex (a + i + 1) (b + l)
                              ,dungeoN.isIndex (a + i - 1) (b + l + 1)
                              ,dungeoN.isIndex (a + i)     (b + l + 1)
                              ,dungeoN.isIndex (a + i + 1) (b + l + 1)) with
                        | (aa,ab,ac,ba,bc,ca,cb,cc) ->
                            if ab
                                then
                                    let newChunkMap = this.dungeon.[this.dungeonIndex].chunkMap
                                    let list = 
                                        [|for p in 0 .. 15 -> 
                                            [|for q in 0 .. 3 -> 
                                                if (q = 0) || (q = 1)
                                                    then (newChunkMap.[(a + i),(b + l - 1)]).getMapData p (q + 14)
                                                    else (newChunkMap.[(a + i),(b + l)]).getMapData p (q - 2)
                                            |]
                                        |]
                                    for q in 1 .. 14 do 
                                        newChunkMap.[(a + i),(b + l - 1)].setTile (idToTile (tileCatToId list q 1)) q 15
                                        newChunkMap.[(a + i),(b + l)].setTile (idToTile (tileCatToId list q 2)) q 0
                                    dungeoN.chunkMap <- newChunkMap
                                    this.dungeon.[this.dungeonIndex] <- dungeoN
                            if cb
                                then
                                    let newChunkMap = this.dungeon.[this.dungeonIndex].chunkMap
                                    let list = 
                                        [|for p in 0 .. 15 -> 
                                            [|for q in 0 .. 3 -> 
                                                if (q = 0) || (q = 1)
                                                    then (newChunkMap.[(a + i),(b + l)]).getMapData p (q + 14)
                                                    else (newChunkMap.[(a + i),(b + l + 1)]).getMapData p (q - 2)
                                            |]
                                        |]
                                    for q in 1 .. 14 do 
                                        newChunkMap.[(a + i),(b + l)].setTile (idToTile (tileCatToId list q 1)) q 15
                                        newChunkMap.[(a + i),(b + l + 1)].setTile (idToTile (tileCatToId list q 2)) q 0
                                    dungeoN.chunkMap <- newChunkMap
                                    this.dungeon.[this.dungeonIndex] <- dungeoN
                            if ba
                                then
                                    let newChunkMap = this.dungeon.[this.dungeonIndex].chunkMap
                                    let list = 
                                        [|for p in 0 .. 3 -> 
                                            [|for q in 0 .. 15 -> 
                                                if (p = 0) || (p = 1)
                                                    then (newChunkMap.[(a + i - 1),(b + l)]).getMapData (p + 14) q
                                                    else (newChunkMap.[(a + i),(b + l)]).getMapData (p - 2) q
                                            |]
                                        |]
                                    for q in 1 .. 14 do 
                                        newChunkMap.[(a + i - 1),(b + l)].setTile (idToTile (tileCatToId list 1 q)) 15 q
                                        newChunkMap.[(a + i),(b + l)].setTile (idToTile (tileCatToId list 2 q)) 0 q
                                    dungeoN.chunkMap <- newChunkMap
                                    this.dungeon.[this.dungeonIndex] <- dungeoN
                            if bc
                                then
                                    let newChunkMap = this.dungeon.[this.dungeonIndex].chunkMap
                                    let list = 
                                        [|for p in 0 .. 3 -> 
                                            [|for q in 0 .. 15 -> 
                                                if (p = 0) || (p = 1)
                                                    then (newChunkMap.[(a + i),(b + l)]).getMapData (p + 14) q
                                                    else (newChunkMap.[(a + i + 1),(b + l)]).getMapData (p - 2) q
                                            |]
                                        |]
                                    for q in 1 .. 14 do 
                                        newChunkMap.[(a + i),(b + l)].setTile (idToTile (tileCatToId list 1 q)) 15 q
                                        newChunkMap.[(a + i + 1),(b + l)].setTile (idToTile (tileCatToId list 2 q)) 0 q
                                    dungeoN.chunkMap <- newChunkMap
                                    this.dungeon.[this.dungeonIndex] <- dungeoN
                            if (aa && ab && ba)
                                then
                                    let newChunkMap = this.dungeon.[this.dungeonIndex].chunkMap
                                    let list = 
                                        [|for p in 0 .. 3 -> 
                                            [|for q in 0 .. 3 -> 
                                                match (p < 2, q < 2) with
                                                | (true,  true ) -> (newChunkMap.[(a + i - 1),(b + l - 1)]).getMapData (p + 14) (q + 14)
                                                | (true,  false) -> (newChunkMap.[(a + i - 1),    (b + l)]).getMapData (p + 14) (q - 2)
                                                | (false, true ) -> (newChunkMap.[(a + i),    (b + l - 1)]).getMapData (p - 2) (q + 14)
                                                | (false, false) -> (newChunkMap.[(a + i),        (b + l)]).getMapData (p - 2) (q - 2)
                                            |]
                                        |]

                                    newChunkMap.[(a + i - 1),(b + l - 1)].setTile (idToTile (tileCatToId list 1 1)) 15 15
                                    newChunkMap.[(a + i - 1),    (b + l)].setTile (idToTile (tileCatToId list 1 2)) 15 0
                                    newChunkMap.[(a + i),    (b + l - 1)].setTile (idToTile (tileCatToId list 2 1)) 0 15
                                    newChunkMap.[(a + i),        (b + l)].setTile (idToTile (tileCatToId list 2 2)) 0 0

                                    dungeoN.chunkMap <- newChunkMap
                                    this.dungeon.[this.dungeonIndex] <- dungeoN
                            if (ac && ab && bc)
                                then
                                    let newChunkMap = this.dungeon.[this.dungeonIndex].chunkMap
                                    let list = 
                                        [|for p in 0 .. 3 -> 
                                            [|for q in 0 .. 3 -> 
                                                match (p < 2, q < 2) with
                                                | (true,  true ) -> (newChunkMap.[(a + i),    (b + l - 1)]).getMapData (p + 14) (q + 14)
                                                | (true,  false) -> (newChunkMap.[(a + i),        (b + l)]).getMapData (p + 14) (q - 2)
                                                | (false, true ) -> (newChunkMap.[(a + i + 1),(b + l - 1)]).getMapData (p - 2) (q + 14)
                                                | (false, false) -> (newChunkMap.[(a + i + 1),    (b + l)]).getMapData (p - 2) (q - 2)
                                            |]
                                        |]

                                    newChunkMap.[(a + i),    (b + l - 1)].setTile (idToTile (tileCatToId list 1 1)) 15 15
                                    newChunkMap.[(a + i),        (b + l)].setTile (idToTile (tileCatToId list 1 2)) 15 0
                                    newChunkMap.[(a + i + 1),(b + l - 1)].setTile (idToTile (tileCatToId list 2 1)) 0 15
                                    newChunkMap.[(a + i + 1),    (b + l)].setTile (idToTile (tileCatToId list 2 2)) 0 0

                                    dungeoN.chunkMap <- newChunkMap
                                    this.dungeon.[this.dungeonIndex] <- dungeoN

                            if (ca && ba && cb)
                                then
                                    let newChunkMap = this.dungeon.[this.dungeonIndex].chunkMap
                                    let list = 
                                        [|for p in 0 .. 3 -> 
                                            [|for q in 0 .. 3 -> 
                                                match (p < 2, q < 2) with
                                                | (true,  true ) -> (newChunkMap.[(a + i - 1),    (b + l)]).getMapData (p + 14) (q + 14)
                                                | (true,  false) -> (newChunkMap.[(a + i - 1),(b + l + 1)]).getMapData (p + 14) (q - 2)
                                                | (false, true ) -> (newChunkMap.[(a + i),        (b + l)]).getMapData (p - 2) (q + 14)
                                                | (false, false) -> (newChunkMap.[(a + i),    (b + l + 1)]).getMapData (p - 2) (q - 2)
                                            |]
                                        |]

                                    newChunkMap.[(a + i - 1),    (b + l)].setTile (idToTile (tileCatToId list 1 1)) 15 15
                                    newChunkMap.[(a + i - 1),(b + l + 1)].setTile (idToTile (tileCatToId list 1 2)) 15 0
                                    newChunkMap.[(a + i),        (b + l)].setTile (idToTile (tileCatToId list 2 1)) 0 15
                                    newChunkMap.[(a + i),    (b + l + 1)].setTile (idToTile (tileCatToId list 2 2)) 0 0

                                    dungeoN.chunkMap <- newChunkMap
                                    this.dungeon.[this.dungeonIndex] <- dungeoN
                            if (cc && cb && bc)
                                then
                                    let newChunkMap = this.dungeon.[this.dungeonIndex].chunkMap
                                    let list = 
                                        [|for p in 0 .. 3 -> 
                                            [|for q in 0 .. 3 -> 
                                                match (p < 2, q < 2) with
                                                | (true,  true ) -> (newChunkMap.[(a + i),        (b + l)]).getMapData (p + 14) (q + 14)
                                                | (true,  false) -> (newChunkMap.[(a + i),    (b + l + 1)]).getMapData (p + 14) (q - 2)
                                                | (false, true ) -> (newChunkMap.[(a + i + 1),    (b + l)]).getMapData (p - 2) (q + 14)
                                                | (false, false) -> (newChunkMap.[(a + i + 1),(b + l + 1)]).getMapData (p - 2) (q - 2)
                                            |]
                                        |]

                                    newChunkMap.[(a + i),        (b + l)].setTile (idToTile (tileCatToId list 1 1)) 15 15
                                    newChunkMap.[(a + i),    (b + l + 1)].setTile (idToTile (tileCatToId list 1 2)) 15 0
                                    newChunkMap.[(a + i + 1),    (b + l)].setTile (idToTile (tileCatToId list 2 1)) 0 15
                                    newChunkMap.[(a + i + 1),(b + l + 1)].setTile (idToTile (tileCatToId list 2 2)) 0 0

                                    dungeoN.chunkMap <- newChunkMap
                                    this.dungeon.[this.dungeonIndex] <- dungeoN

        this.chunks <- match this.chunkXY with
                        | (a, b) ->
                        [|for p in -1 .. 1 ->
                            [|for q in -1 .. 1 ->
                                this.dungeon.[this.dungeonIndex].chunkMap.[(a + p),(b + q)]
                            |]
                        |]

    member this.drawGame (evArgs : PaintEventArgs) (size : Size) =
        let drawBrushB = new SolidBrush (Color.Black)
        let drawBrushW = new SolidBrush (Color.White)
        let drawPen = new Pen (drawBrushB)
        let drawFont = new Font ("MSゴシック", 10f)
        let drawString = 
            "cam_XY = " + this.cam.ToString () + 
            "\nchunkXY = " + this.chunkXY.ToString () +
            "\nseed = " + this.seed.ToString () + 
            "\nstate = " + this.state.ToString ()

        for i: int32 in 0 .. 2 do
            for l: int32 in 0 .. 2 do
                match this.cam with
                | (a: int,b: int) -> this.chunks.[i].[l].drawChunk evArgs ((i - 1) * 16 - a % 16) ((l - 1) * 16 - b % 16) 0 0
        match this.mode with
        | Debug   -> 
            evArgs.Graphics.DrawString (drawString,drawFont, drawBrushW,new PointF ())
        | Command -> 
            evArgs.Graphics.FillRectangle (drawBrushB, new Rectangle (0,size.Height - 20 ,size.Width,size.Height))
            evArgs.Graphics.DrawString (
                if this.text.Length = 0 
                    then "" 
                    else this.text.Substring (1) 
                , drawFont, drawBrushW, new PointF (0f, float32 size.Height - 20f)
            )
        | _       -> ()

    member private this.setChunkXY =
        match this.cam with
        | (a, b) ->
            this.chunkXY <- (a / 16, b / 16)

    member this.move x y =
        match this.cam with
        | (a, b) -> 
            this.cam <- (a + x, b + y)
        this.setChunkXY
    
    member this.keyPless (ev : KeyPressEventArgs) = (
        if this.mode = Command
            then match ev.KeyChar with
                    | '\b' -> ()
                    | '\n' -> ()
                    | _    -> this.text <- this.text + ev.KeyChar.ToString ()
    )

    member this.keyMove (ev : KeyEventArgs, form : Form) =
        if this.state = Stay
            then if this.mode = Command then (
                    match ev.KeyCode with
                    | Keys.Escape -> (
                        this.mode <- Neutral
                        this.text <- ""
                        )
                    | Keys.Back -> (
                        if this.text.Length > 1 then this.text <- this.text.Substring(0, this.text.Length - 1)
                        )
                    | Keys.Enter -> (
                        let com: Command = new Command (this.text.Substring(1))
                        this.cam <- com.execute
                        System.Diagnostics.Debug.WriteLine(com.debug)
                        System.Diagnostics.Debug.WriteLine(com.operand = "tp")
                        System.Diagnostics.Debug.WriteLine(com.execute.ToString ())
                        this.text <- ""
                        ) 
                    | _ -> ()
                ) 
            else ( match ev.KeyCode with
                        | Keys.W -> (
                            this.state <- Moving
                            )
                        | Keys.A -> (
                            this.state <- Moving
                            )
                        | Keys.X -> (
                            this.state <- Moving
                            )
                        | Keys.D -> (
                            this.state <- Moving
                            )
                        | Keys.Q -> (
                            this.state <- Moving
                            )
                        | Keys.E -> (
                            this.state <- Moving
                            )
                        | Keys.Z -> (
                            this.state <- Moving
                            )
                        | Keys.C -> (
                            this.state <- Moving
                            )
                        | Keys.T -> (
                            this.mode <- Command
                            )
                        | Keys.Escape -> form.Close ()
                        | Keys.F3 -> this.mode <- Debug
                        | _ -> ()
            )

    end
