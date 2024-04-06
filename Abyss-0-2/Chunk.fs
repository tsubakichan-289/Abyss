module Chunk

open System.Windows.Forms
open System.Drawing
open Tile
open Biome

type ChunkXY = class
    val x : int
    val y : int
    new (_x: int, _y: int) = {
        x = _x
        y = _y
    }
    end

type Chunk = class
    val map : array<array<TileCategory>>
    val mutable tileIds : array<array<int>>
    //val biome : Biome

    new (mapFunc, (chunkXY : ChunkXY)) =
        let map: TileCategory array array = [|
            for i: int in chunkXY.x * 16 .. (chunkXY.x + 1) * 16 - 1 -> [|
                for l: int in chunkXY.y * 16 .. (chunkXY.y + 1) * 16 - 1 -> Tile.floatToCategory (mapFunc i l)
            |]
        |]
        {
            map = map
            tileIds = [|
                for i: int in 0 .. 15 -> [|
                    for l: int in 0 .. 15 -> 
                        match map.[i % 16].[l % 16] with 
                        | Yuka -> 29
                        | Kabe -> 45
                        | Ana -> 46
                        | Mizu -> 48
                |]
            |]
        }

    member this.getMapData (x: int) (y: int) = this.map.[x % 16].[y % 16]

    member this.drawChunk (evArgs : PaintEventArgs) (x: int32) (y: int32) (dx: int) (dy: int) = 
        for i: int32 in 0 .. 15 do 
            for l: int32 in 0 .. 15 do 
                (tiles.[this.tileIds.[i].[l]]).drawTile2 evArgs (x + i) (y + l) dx dy
    
    member this.setTile (tile: int) (x: int) (y: int) = this.tileIds.[x].[y] <- tile

    member this.setTiles = 
        this.tileIds <- 
            [|for i: int in 0 .. 15 
                ->
                [|for l: int in 0 .. 15 
                    -> match this.getMapData (i) (l) with 
                        | Yuka -> 29
                        | Kabe -> 45
                        | Ana -> 46
                        | Mizu -> 48

        |]
    |]

    end
   