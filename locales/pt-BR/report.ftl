report-lock-started = O bloqueio { $label } começou.
report-lock-ended = O bloqueio { $label } terminou.
report-extension = Você pediu mais { $minutes } min durante o bloqueio { $label }. Seu motivo: { $reason }
report-extension-terminal = Você pediu mais { $minutes } min pelo terminal durante o bloqueio { $label }. Seu motivo: { $reason }
report-shutdown = O computador desligou durante o bloqueio { $label }.
report-rule-shutdown = O computador desligou pela regra { $label }.
report-exception = Você deixou a regra { $label } de fora pelo resto do dia. Seu motivo: { $reason }
report-exception-plain = Você deixou a regra { $label } de fora pelo resto do dia.
report-lock-started-test = O bloqueio de teste "{ $label }" começou.
report-lock-ended-test = O bloqueio de teste "{ $label }" terminou.
report-extension-test = Você pediu mais { $minutes } min durante o bloqueio de teste "{ $label }". Seu motivo: { $reason }
report-extension-terminal-test = Você pediu mais { $minutes } min pelo terminal durante o bloqueio de teste "{ $label }". Seu motivo: { $reason }
report-config-saved-tighten = Você salvou uma mudança que deixa as regras mais firmes. Ela passou a valer na hora.
report-config-saved-loosen = Você salvou uma mudança que afrouxa as regras. Ela espera um tempo antes de valer.
report-config-saved-neutral = Você salvou suas configurações.
report-config-saved = Você salvou suas configurações (mudanças que valeram na hora: { $now }, mudanças aguardando: { $waiting }).
report-agent-message-inbox =
    O Selflow enviou a mensagem da regra { $label } à sessão do Claude Code em { $folder } pela caixa de entrada. Nível: { $level ->
        [message] Só mensagem
        [light] Só tarefas leves
        [no-new-work] Nada novo
       *[save-only] Só salvar
    }.
report-agent-message-prompt =
    O Selflow entregou a mensagem da regra { $label } à sessão do Claude Code em { $folder } junto com o próximo prompt. Nível: { $level ->
        [message] Só mensagem
        [light] Só tarefas leves
        [no-new-work] Nada novo
       *[save-only] Só salvar
    }.
report-agent-message-missed =
    A sessão do Claude Code em { $folder } terminou antes de receber a mensagem da regra { $label }. Nível: { $level ->
        [message] Só mensagem
        [light] Só tarefas leves
        [no-new-work] Nada novo
       *[save-only] Só salvar
    }.
report-agent-denied-edit =
    O Selflow negou uma edição de arquivo na sessão do Claude Code em { $folder } durante a regra { $label }. Nível: { $level ->
        [message] Só mensagem
        [light] Só tarefas leves
        [no-new-work] Nada novo
       *[save-only] Só salvar
    }.
report-agent-denied-shell =
    O Selflow negou um comando de shell na sessão do Claude Code em { $folder } durante a regra { $label }. Nível: { $level ->
        [message] Só mensagem
        [light] Só tarefas leves
        [no-new-work] Nada novo
       *[save-only] Só salvar
    }.
report-agent-unattended =
    Uma sessão do Claude Code sem supervisão começou em { $folder } durante a regra { $label }. Nível: { $level ->
        [message] Só mensagem
        [light] Só tarefas leves
        [no-new-work] Nada novo
       *[save-only] Só salvar
    }.
