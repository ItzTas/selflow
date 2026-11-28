change-rule-added = A regra `{ $rule }` passa a valer em { $days }.
change-rule-removed = A regra `{ $rule }` deixa de valer em { $days }.
change-rule-starts-earlier = A regra `{ $rule }` começa mais cedo em { $days }: { $old } → { $new }.
change-rule-starts-later = A regra `{ $rule }` começa mais tarde em { $days }: { $old } → { $new }.
change-rule-ends-later = A regra `{ $rule }` termina mais tarde em { $days }: { $old } → { $new }.
change-rule-ends-earlier = A regra `{ $rule }` termina mais cedo em { $days }: { $old } → { $new }.
change-rule-timing = Os horários da regra `{ $rule }` estão escritos de outro jeito em { $days }.
change-canceled = Uma mudança que aguardava na regra `{ $rule }` em { $days } não vai acontecer.
change-lock-added = Em { $days }, a regra `{ $rule }` passa a bloquear a tela.
change-lock-removed = Em { $days }, a regra `{ $rule }` deixa de bloquear a tela.
change-shutdown-added = Em { $days }, o computador desliga quando o bloqueio de `{ $rule }` chega a { $new }.
change-shutdown-removed = Em { $days }, o bloqueio de `{ $rule }` deixa de desligar o computador depois de { $old }.
change-shutdown-sooner = Em { $days }, o bloqueio de `{ $rule }` desliga o computador mais cedo: { $old } → { $new }.
change-shutdown-later = Em { $days }, o bloqueio de `{ $rule }` desliga o computador mais tarde: { $old } → { $new }.
change-shutdown-at-start-added = Em { $days }, a regra `{ $rule }` passa a desligar o computador quando começa.
change-shutdown-at-start-removed = Em { $days }, a regra `{ $rule }` deixa de desligar o computador quando começa.
change-shutdown-repeats = Em { $days }, a regra `{ $rule }` desliga o computador de novo a cada vez que ele liga, até a regra terminar.
change-shutdown-once = Em { $days }, a regra `{ $rule }` desliga o computador uma vez só, e não de novo quando ele liga.
change-notices = Os avisos da regra `{ $rule }` mudam em { $days }: { $old } → { $new }.
change-message = A mensagem da regra `{ $rule }` muda em { $days }.
change-label = O nome mostrado para a regra `{ $rule }` muda em { $days }: { $old } → { $new }.
change-wait-longer = Na regra `{ $rule }`, a espera antes de desbloquear fica maior em { $days }: { $old } → { $new }.
change-wait-shorter = Na regra `{ $rule }`, a espera antes de desbloquear fica menor em { $days }: { $old } → { $new }.
change-reason-required = Na regra `{ $rule }`, desbloquear pede um motivo escrito em { $days }.
change-reason-dropped = Na regra `{ $rule }`, desbloquear deixa de pedir um motivo escrito em { $days }.
change-phrase-added = Na regra `{ $rule }`, desbloquear pede que você digite uma frase em { $days }: { $new }
change-phrase-dropped = Na regra `{ $rule }`, desbloquear deixa de pedir que você digite uma frase em { $days }.
change-phrase-changed = Na regra `{ $rule }`, a frase a digitar muda em { $days }: { $old } → { $new }
change-fewer-extensions = Na regra `{ $rule }`, cada dia permite menos extensões em { $days }: { $old } → { $new }.
change-more-extensions = Na regra `{ $rule }`, cada dia permite mais extensões em { $days }: { $old } → { $new }.
change-shorter-extensions = Na regra `{ $rule }`, cada extensão fica mais curta em { $days }: { $old } → { $new }.
change-longer-extensions = Na regra `{ $rule }`, cada extensão fica mais longa em { $days }: { $old } → { $new }.
change-day-start = O dia começa em outro horário: { $old } → { $new }.
change-delay-longer = Mudanças que afrouxam as regras esperam mais antes de valer: { $old } → { $new }.
change-delay-shorter = Mudanças que afrouxam as regras esperam menos antes de valer: { $old } → { $new }.
change-setting-canceled = Uma mudança que aguardava em `{ $setting }` não vai acontecer.
change-language = O idioma muda: { $old } → { $new }.
change-exception-added =
    { $rules ->
        [all] Em { $date }, nenhuma regra acontece.
       *[other] Em { $date }, estas regras não acontecem: { $rules }.
    }
change-exception-removed =
    { $rules ->
        [all] Em { $date }, todas as regras voltam a acontecer.
       *[other] Em { $date }, estas regras voltam a acontecer: { $rules }.
    }
change-exception-wider =
    { $rules ->
        [all] A exceção para { $date } agora vale para todas as regras.
       *[other] A exceção para { $date } agora vale para mais regras: { $rules }.
    }
change-exception-narrower = A exceção para { $date } agora vale para menos regras: { $rules }.
change-exception-changed = A exceção para { $date } agora vale para outras regras: { $rules }.
change-exception-canceled = Uma mudança que aguardava na exceção para { $date } não vai acontecer.
change-agent-step-added = Na regra `{ $rule }`, um passo do Claude Code é adicionado em { $days }: { $new }.
change-agent-step-removed = Na regra `{ $rule }`, um passo do Claude Code é removido em { $days }: { $old }.
change-agent-step-firmer = Na regra `{ $rule }`, um passo do Claude Code fica mais firme em { $days }: { $old } → { $new }.
change-agent-step-looser = Na regra `{ $rule }`, um passo do Claude Code fica mais frouxo em { $days }: { $old } → { $new }.
change-agent-step-changed = Na regra `{ $rule }`, um passo do Claude Code muda em { $days }: { $old } → { $new }.
change-agent-message = Na regra `{ $rule }`, a mensagem do passo ({ $step }) do Claude Code muda em { $days }.
change-agent-commands-fewer = No nível `save-only`, as sessões do Claude Code podem rodar menos comandos: { $old } → { $new }.
change-agent-commands-more = No nível `save-only`, as sessões do Claude Code podem rodar mais comandos: { $old } → { $new }.
change-agent-commands-changed = Os comandos que as sessões do Claude Code podem rodar no nível `save-only` mudam: { $old } → { $new }.
