open System.Windows.Forms
open System.Drawing
open Tile
open Noise
open Chunk
open Dungeon
open Game
open Debug

let game =
    let g = new Game 1024
    g.setChunks
    g

let pBox: PictureBox = 
    let p: PictureBox = new PictureBox ()
    p.Size <- new Size (960,960)
    p

let form: Form =
    let f: Form = new Form ()
    f.Text <- "Abyss-0-1"
    f.ClientSize <- new Size (960,960)
    f.Controls.Add pBox
    f.BackColor <- Color.Black
    f.KeyDown.Add (fun (ev: KeyEventArgs) -> (
        game.keyMove (ev, f)
    ))
    f.KeyPress.Add (fun (ev: KeyPressEventArgs) -> (
        debug "aaaaaaaaaaaaaaaaaaaaaaaa"
        game.keyPless ev
    ))
    f.Show ()
    f

let timer =
    let t = new Timer ()
    t.Interval <- 1000 / 16
    t.Enabled <- true
    t.Tick.Add(fun evArgs -> 
        pBox.Size <- new Size (15 * (64 * form.ClientSize.Width / 960) ,15 * (64 * form.ClientSize.Height / 960))
        game.tick
        pBox.Refresh ())
    t.Start ()
    t

[<EntryPoint>]
let main (argv: string array) =
    pBox.Paint.Add (fun (ev: PaintEventArgs) -> game.drawGame ev (Size (10, 10)))
    Application.Run form
    0