module Dungeon

open System.Drawing
open System.Media

open Noise
open Chunk


type Dungeon = class
    val mutable chunkMap : Map<(int*int),Chunk>
    val noise : (PerlinNoise*PerlinNoise)
    val seed : int

    new (_seed: int) = {
        seed = _seed 
        noise = (new PerlinNoise (_seed),new PerlinNoise (_seed + 88182))
        //bgm = 
        chunkMap = Map []
    }



    member this.isIndex (a: int) (b: int) =
        Map.containsKey (a, b) this.chunkMap

    member this.addChunk (chunkX: int) (chunkY: int) =
        let newChunk: Chunk = new Chunk ( match this.noise with
                                            | (a, b) -> fun x y -> (a.mainMap x y, b.mainMap x y)
        
        , new ChunkXY (chunkX, chunkY))
        newChunk.setTiles
        this.chunkMap <- this.chunkMap.Add ((chunkX, chunkY), newChunk)


    end