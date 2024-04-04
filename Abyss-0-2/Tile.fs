module Tile

open System.IO
open System.Windows.Forms
open System.Drawing
open System
open System.Diagnostics

type TileCategory = Yuka | Kabe | Ana

type Tile = class
    val frames : Image array
    val mutable size : Size
    val mutable private frameNum : int
    
    new (path: string) = {
        frames =
            let p_path: string array = Directory.GetFiles path
            [|for frame: string in p_path ->
                Image.FromFile frame
            |]
        size = Size (960,960)
        frameNum = 0
    }

    member private this.maxFrame = this.frames.Length

    member this.nextFrame =
        if this.frameNum + 1 = this.maxFrame
        then this.frameNum <- 0
        else this.frameNum <- this.frameNum + 1

    member this.drawTile1 (evArgs : PaintEventArgs) (x: int) (y: int) = 
        evArgs.Graphics.DrawImage (this.frames[this.frameNum], float32 (x * 64 * this.size.Width / 960), float32 (y * 64 * this.size.Height / 960), float32 (64 * this.size.Width / 960), float32 (64 * this.size.Height / 960))
    
    member this.drawTile2 (evArgs : PaintEventArgs) (x: int) (y: int) (dx: int) (dy: int) = 
        evArgs.Graphics.DrawImage (this.frames[this.frameNum], float32 (dx + x * 64 * this.size.Width / 960), float32 (dy + y * 64 * this.size.Height / 960), float32 (64 * this.size.Width / 960), float32 (64 * this.size.Height / 960))

    end



let tiles: Tile array =
    let paths: string array = Directory.GetDirectories "src\pictures"

    [|for path: string in paths ->
        Tile path
    |]

let idToTile (i:int) = 
    let idString: string = i.ToString ()
    let name: Tile = Tile ("src\pictures\\" + (System.String [|for q in 1 .. (5 - idString.Length) -> '0' |]) + idString)
    name
    

let floatToTile (f: float) =
    match (f < 40.0, f < 50.0) with
    | (true,  true ) -> tiles[0]
    | (false, true ) -> tiles[1]
    | (false, false) -> tiles[2]
    | _              -> tiles[0]

let floatToId (f: float) = 
    match (f < 40.0, f < 45.0) with
    | (true,  true ) -> 0
    | (false, true ) -> 1
    | (false, false) -> 2
    | _              -> 0

let floatToCategory (f: float) = 
    match (f < 45.0, f < 55.0) with
    | (true,  true ) -> Kabe
    | (false, true ) -> Yuka
    | (false, false) -> Ana
    | _              -> Ana

let tileCatToId (tiles : array<array<TileCategory>>) i l = 
    match tiles.[i].[l] with
    | Yuka -> 29
    | Kabe -> 45
    | Ana -> 46