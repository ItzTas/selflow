config-syntax = Não foi possível ler esta parte do arquivo: { $detail }
config-bad-duration = O valor `{ $value }` não é uma duração como 15m ou 2h 30m.
config-not-positive = A duração `{ $value }` precisa ser maior que zero.
config-bad-time = O valor `{ $value }` não é um horário como 23:00.
config-unknown-anchor = Não há nenhuma âncora chamada `{ $name }` em [anchors].
config-unknown-language = O idioma `{ $value }` não está disponível. As opções são "en" e "pt-BR".
config-bad-day = O valor `{ $value }` não é um dia. Os dias são mon, tue, wed, thu, fri, sat e sun.
config-day-in-two-sets = O dia `{ $day }` está em `{ $first }` e em `{ $second }`. Cada dia pertence a um só conjunto de regras.
config-duplicate-set = Já existe um conjunto de regras chamado `{ $name }`.
config-duplicate-rule = O conjunto de regras `{ $set }` já tem uma regra com o id `{ $id }`.
config-empty-window = A regra `{ $id }` começa e termina no mesmo horário.
config-jitter-too-long = O start_jitter da regra `{ $id }` precisa ser menor que a própria regra.
config-too-long = A regra `{ $id }` precisa durar menos de um dia, contando o length e o start_jitter, se houver.
config-locks-overlap = No conjunto de regras `{ $set }`, os bloqueios das regras `{ $first }` e `{ $second }` se sobrepõem.
config-negative-offset = O tempo `{ $value }` não pode ser negativo. Use 0m para bem no início.
config-end-and-length = A regra `{ $id }` tem end e length ao mesmo tempo. Fique com o que você quer dizer.
config-lock-needs-end = A regra `{ $id }` bloqueia a tela, então precisa de um end ou de um length para dizer quando o bloqueio acaba.
config-shutdown-after-needs-lock = A regra `{ $id }` tem shutdown_after mas não tem bloqueio. Adicione lock = true, ou use shutdown para desligar no início.
config-shutdown-with-lock = A regra `{ $id }` tem lock e shutdown ao mesmo tempo. Com um bloqueio, use shutdown_after.
config-bad-shutdown = O valor `{ $value }` não é um jeito de desligar. As opções são "once" e "repeat".
config-bad-message = A mensagem da regra `{ $id }` precisa ser um texto ou uma lista de textos.
config-empty-message = A mensagem da regra `{ $id }` tem um texto vazio.
config-bad-date = O valor `{ $value }` não é uma data como 2026-12-25.
config-duplicate-exception = Já existe uma exceção para { $date }.
config-exception-unknown-rule = A exceção para { $date } cita `{ $rule }`, mas o conjunto de regras desse dia não tem nenhuma regra com esse id.
config-agent-bad-level = O nível `{ $value }` num passo do Claude da regra `{ $id }` não é um que o Selflow conhece. Os níveis são message, light, no-new-work e save-only.
config-agent-bad-folder = A pasta `{ $value }` num passo do Claude da regra `{ $id }` precisa ser `~`, começar com `~/` ou ser um caminho absoluto. Nenhuma das partes dela pode ser `.` ou `..`.
config-agent-duplicate-step = A regra `{ $id }` tem dois passos do Claude com before = `{ $before }` e nível `{ $level }`. Remova um deles ou mude o before ou o nível.
config-agent-too-early = Um passo do Claude da regra `{ $id }` tem before = `{ $value }`. Ele precisa ser menor que 24h.
config-agent-whole-minutes = Um passo do Claude da regra `{ $id }` tem before = `{ $value }`. Use minutos inteiros, como 1m ou 90m.
config-agent-empty-command = Uma entrada de allowed_commands em [agent] está vazia. Escreva um comando como "git status" ou remova a entrada.
config-before-update = Suas configurações de antes desta atualização do Selflow, guardadas aqui para consulta. O Selflow não as lê mais, então copie para o arquivo acima o que quiser manter.
