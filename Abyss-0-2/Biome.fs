module Biome

open System.IO

let readCsvWithStandardLibrary (filePath: string) =
    let lines = File.ReadAllLines(filePath)
    lines
    |> Array.map (fun line -> line.Split(','))
    // ここで各行に対して何か処理を行う
    |> Array.iter (fun fields ->
        // fieldsはここで各行のカラム配列
        printfn "%A" fields)

type Biome = class 
    val name : string

    new (biomeID) = {
        name = match biomeID with
               | (0, 0) -> "Valley of Timeless"
               | (0, 1) -> "Starfall Valley"
               | (0, 2) -> "Mirage Desert"
               | (0, 3) -> "Mist-shrouded Forest"
               | (0, 4) -> "Mystical Falls"
               | (0, 5) -> "Permafrost Edge"
               | (1, 0) -> "Starfall Valley"
               | (1, 1) -> "Luminescent Peaks"
               | (1, 2) -> "Spectral Marsh"
               | (1, 3) -> "Tranquil Lakeside"
               | (1, 4) -> "Rugged Mountains"
               | (1, 5) -> "Sea of Forgetfulness"
               | (2, 0) -> "Silverleaf Forest"
               | (2, 1) -> "Ancient Ruins"
               | (2, 2) -> "Lush Forest"
               | (2, 3) -> "Vast Meadow"
               | (2, 4) -> "Hidden Light Forest"
               | (2, 5) -> "Echoing Plains"
               | (3, 0) -> "Crystal Cavern"
               | (3, 1) -> "Tranquil Lakeside"
               | (3, 2) -> "Vast Meadow"
               | (3, 3) -> "Scorching Wind Desert"
               | (3, 4) -> "Gemstone Sea"
               | (3, 5) -> "Golden Shores"
               | (4, 0) -> "Tornado Plains"
               | (4, 1) -> "Rugged Mountains"
               | (4, 2) -> "Stone Forest"
               | (4, 3) -> "Frosted Wood"
               | (4, 4) -> "Cliffside Gorge"
               | (4, 5) -> "Shadow Realm"
               | (5, 0) -> "Scorching Wind Desert"
               | (5, 1) -> "Barren Canyon"
               | (5, 2) -> "Emerald Springs"
               | (5, 3) -> "Fiery Canyon"
               | (5, 4) -> "Shadow Realm"
               | (5, 5) -> "Starlit Garden"
               | _      -> ""
    }

    end