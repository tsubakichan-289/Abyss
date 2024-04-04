module Dungeon

open System.Drawing
open System.Media

open Noise
open Chunk


type Dungeon = class
    val mutable chunkMap : Map<(int*int),Chunk>
    val noise : PerlinNoise
    val seed : int

    new (_seed: int) = {
        seed = _seed 
        noise = new PerlinNoise (_seed)
        //bgm = 
        chunkMap = Map []
    }



    member this.isIndex (a: int) (b: int) =
        Map.containsKey (a, b) this.chunkMap

    member this.addChunk (chunkX: int) (chunkY: int) =
        let newChunk: Chunk = new Chunk (this.noise.mainMap, new ChunkXY (chunkX, chunkY))
        newChunk.setTiles
        this.chunkMap <- this.chunkMap.Add ((chunkX, chunkY), newChunk)


    end