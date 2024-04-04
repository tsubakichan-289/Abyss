module Command

type Command =
    class
        
        val operand : string
        val immediate : list<string>

        new (input : string) = 
            match (List.ofArray (input.Split (' '))) with
            | _operand::_immediate -> {
                    operand = _operand
                    immediate = _immediate
                }
            | _                    -> {
                    operand = ""
                    immediate = []
                }

        member this.debug = "op:" + this.operand.ToString () + " im:" + this.immediate.ToString ()

        member this.execute =
            match this.operand with
            | "tp" -> (int this.immediate.[0],int this.immediate.[1])
            | _    -> (0,0)

    end