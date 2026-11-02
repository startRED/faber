# 1. Resumo

O super-agente propõe uma conversa principal que organiza o trabalho e distribui tarefas para outras sessões de IA.
Essas sessões trabalham separadas, devolvem resultados e podem receber acompanhamento sem prender a conversa principal.
Eventos externos podem sugerir tarefas; filtros e um monitor tentam reduzir ruído e detectar trabalho travado.
O PandaCode reúne agentes, conversas entre bots e tarefas agendadas; o Cline Desktop oferece sessões paralelas e agendamentos.
O ClinePass é uma assinatura separada para vários modelos; o Faber usa as assinaturas que a pessoa já tem do Claude Code e do Codex.

A pesquisa descreve um protótipo experimental; os testes publicados são pequenos e simulados, então não provam ganho real no Faber. Fontes: [pesquisa das ferramentas](links.md) e [pesquisa do protótipo](dsh.md). A tabela distingue o que existe hoje do que é plano; “não encontrado nas docs” não prova que o recurso não existe no código.

# 2. Comparação com o Faber

| Mecanismo | O que o Faber já tem | O que falta |
|---|---|---|
| Router: conversa que coordena | Auto escolhe modelo e esforço por mensagem; é seleção de modelo, não um coordenador permanente ([mapa](../../../docs/product-map.md), Auto). | Uma sessão duradoura que só conversa, distribui e acompanha; o agente líder é plano da fase 3 ([visão](../../../docs/product/README.md), [construção](../../../docs/product/building.md)). |
| Workers isolados | Subagentes e até dois ajudantes da outra IA por conversa; ajudante tem sessão própria, mas usa a mesma pasta e o mesmo modo de permissão. Missão interna usa cópia separada, com uma tarefa hoje ([mapa](../../../docs/product-map.md); R161, R167). | Coordenação geral de várias tarefas em cópias separadas; sessão ou pasta separada não garante isolamento de segurança. Missões não têm botão para iniciar na tela (R160). |
| Gatilhos + dois filtros | A pessoa inicia conversas e projetos; filtro de eventos externos em duas etapas: não encontrado nas docs. | Receber eventos, barrar remetentes e fontes por regras e depois avaliar relevância com IA, começando apenas por sugestões. |
| Chain of recall | Conversas e sessões são salvas; o diário de missão guarda plano, decisões e relatório ([mapa](../../../docs/product-map.md)). Memória pesquisável e líder com relatórios curtos são planos ([acompanhamento](../../../docs/product/following.md), [construção](../../../docs/product/building.md)). | Resumos ligados por IDs para recuperar a tarefa certa e consultar detalhes sob demanda: não encontrado nas docs como recurso pronto. |
| Watchdog | Detecta sinais de possível travamento de subagentes, inclusive repetição, falhas e falta de atividade; avisa a pessoa (R174, [mapa](../../../docs/product-map.md)). | Monitor independente que avalia progresso real, identifica desvio de objetivo e recupera trabalho automaticamente: não encontrado nas docs. Processo vivo pode estar repetindo sem avançar. |
| Bots conversam entre si | O agente pode chamar ajudante da outra IA e receber relatório; a pessoa pode escrever diretamente nesse ajudante (R161, R196). | Rede de bots permanentes que se enviam tarefas entre si: não encontrada nas docs. |
| Telegram ou chat externo | A visão prevê celular como controle remoto e canais de mensagem na fase 7 ([visão](../../../docs/product/README.md)). | Canal externo pronto: não encontrado nas docs; segurança, identidade e permissões ainda precisam ser definidas. |
| Tarefas recorrentes | Rotinas estão previstas na fase 7 ([visão](../../../docs/product/README.md)). | Agendador de tarefas do usuário pronto: não encontrado nas docs; definir o que acontece com o app ou PC desligado. |
| Assinatura de vários modelos | Usa contas próprias de Claude Code e Codex, escolha de modelo e Auto com acompanhamento de limites ([modelos](../../../docs/product/models.md), [mapa](../../../docs/product-map.md)). | Pacote próprio com modelos de outros fornecedores não faz parte do plano; contraria a regra de usar apenas os dois ([visão](../../../docs/product/README.md)). |
| Contínuo: uma só experiência | Chat e Projetos existem, com conversas por projeto (R160, [mapa](../../../docs/product-map.md)). | Uma mesma conversa que acompanhe tarefas iniciadas em chat, pedidos de alteração de código e outros lugares: não encontrada nas docs; canais externos são futuros. |
| Produtivo: falar durante o trabalho | Mensagens entram em fila; Codex pode receber instrução durante a resposta. Ajudante da outra IA também aceita mensagens, com comportamento diferente em cada ferramenta (R196, [mapa](../../../docs/product-map.md)). | Resposta imediata e alteração em andamento para todos os tipos de sessão e trabalhadores: não encontrada nas docs. |
| Proativo: acompanhar e agir | Alerta de possível travamento já existe (R174); o plano prevê líder que acompanha e tenta outro caminho ([construção](../../../docs/product/building.md)). | Ação proativa geral sobre eventos externos e recuperação por monitor independente: não encontrada nas docs. |
| Persistente: recuperar sozinho | Conversas, fila e registros de missão são salvos ([mapa](../../../docs/product-map.md)). | Retomar automaticamente todas as sessões e tarefas após falha ou reinício é objetivo, não capacidade completa comprovada; ficar online com PC desligado não é prometido ([visão](../../../docs/product/README.md)). |

Há uma divergência nas páginas: [areas.md](../../../docs/product/areas.md) ainda diz que não há conversa direta em projeto, mas o [mapa](../../../docs/product-map.md) e R160 dizem que há. Usei estes últimos, que descrevem a mudança posterior.

# 3. Propostas por valor e esforço

Custos abaixo são expectativas, não medições. Qualquer implementação precisa medir memória e processador; o teste do próprio Faber falha acima de 400 MB sem ferramenta de IA aberta (R125), e sessões de IA também precisam ser medidas separadamente.

1. **Continuar a tarefa certa por resumo e ID. Valor alto, esforço baixo a médio.** A pessoa vê o pedido original, estado e último resultado na conversa existente; ao retomar, Faber abre os detalhes necessários. Evita reler todo o histórico e confundir tarefas. Memória/CPU: baixo para guardar e buscar resumos; gerar resumo consome cota e CPU das ferramentas, a medir. Aproveita conversas e registros existentes; segue as regras de memória por projeto e de passagem por resumo ([modelos](../../../docs/product/models.md), [acompanhamento](../../../docs/product/following.md)).

2. **Recuperação visível e segura. Valor alto, esforço médio.** Após fechar ou travar, a pessoa vê o que terminou, o que espera e o que pode continuar; Faber retoma só o que for seguro, sem repetir envios. Reduz perda de trabalho. Memória/CPU: baixo em espera, gravações pequenas durante o trabalho; retomada abre sessões e consome cota, a medir. Segue o plano de persistência e as permissões de publicação (R82/R83). **Dono decide** o comportamento quando houver dúvida sobre repetir uma ação externa.

3. **Expandir o aviso de travamento atual. Valor alto, esforço médio.** A pessoa vê “parece repetitivo” com o motivo e opção de orientar ou parar; comparar passos e resultados antes de sugerir intervenção. Evita tratar um processo vivo em loop como progresso. Memória/CPU: baixo com contadores; avaliação por IA adicionaria cota e CPU, a medir. Estende R174, sem criar outra tela; mudanças automáticas precisam respeitar permissões (R121) e evitar falsos alarmes. Começar pelo aviso, sem encerrar trabalho longo sozinho.

4. **Líder leve com ajudantes sob demanda. Valor alto, esforço alto.** A pessoa fala com Faber enquanto ele divide trabalho e devolve uma resposta; mostra quem faz o quê na área de trabalho existente. Usa modelos leves já pagos para coordenação e mais fortes quando a tarefa precisa, sujeito à disponibilidade e cota. Memória/CPU: cada sessão adicional pesa; limitar concorrência e encerrar sessões ociosas, medindo o total. Segue R91 e R161 e o orçamento R125; não promete que modelo leve sempre basta. O líder duradouro de fase 3 precisa ser conciliado com a direção de conversa direta de R160/R161. **Dono decide** essa experiência antes da implementação.

5. **Sugestões por eventos, depois rotinas. Valor médio, esforço alto.** Primeiro, Faber mostra “encontrei algo que talvez precise de você”; só age após autorização. Depois, uma rotina pode ser criada com horário e tarefa claros. Memória/CPU: verificação periódica leve, sessões e cota só ao avaliar ou executar; medir, inclusive em segundo plano. Canais e rotinas são fase 7; telemetria continua desligada por padrão, configurações ficam em Configurações ([regras](../../../docs/product-rules.md)). **Dono decide** fontes, frequência, funcionamento com o app fechado e qualquer ação automática ou acesso remoto.

# 4. O que não copiar

- O acesso amplo do protótipo a arquivos e contas diante de mensagens externas: ele reconhece que pastas separadas não são uma barreira de segurança ([pesquisa](dsh.md)).
- Assinatura extra como ClinePass, modelos de outros fornecedores ou guardar chaves de API: contrariam o escopo atual de usar Claude Code e Codex com login próprio ([visão](../../../docs/product/README.md)).
- Empacotar ferramentas de terceiros ou chamar uma função do produto pelo nome de fornecedor: as regras do Faber proíbem isso ([AGENTS](../../../AGENTS.md)).
- Uma rede de bots permanentes só para parecer uma equipe: adiciona processos e consumo sem provar benefício; R125 exige medir e manter o app leve.
- Tratar os números do estudo como promessa: foram simulações pequenas, e o próprio artigo aponta segurança e memória de longo prazo como questões em aberto ([pesquisa](links.md)).

# 5. Segurança

Se um dia Faber aceitar comentários, resultados de testes, Telegram ou outros eventos externos, o conteúdo pode trazer ordens maliciosas disfarçadas de tarefa. Deve tratá-lo como dado, nunca como autorização; conferir origem e remetente, limitar o escopo e registrar de onde veio cada pedido.

Filtros e instruções à IA não bastam: permissões precisam ser aplicadas pelo programa, com isolamento adequado, credenciais protegidas e aprovação para enviar, publicar, pagar ou apagar fora de uma cópia, conforme as regras aplicáveis. R121 dá acesso amplo no modo padrão de conversa; por isso, conectar esse modo automaticamente a eventos externos ampliaria o risco e precisa de decisão explícita do dono. Uma resposta posterior no mesmo assunto não deve ganhar permissões novas por isso.

Na recuperação, guardar o estado antes de agir e conferir se uma ação externa já ocorreu, para evitar envio duplicado; permitir parar tudo e mostrar o histórico. Usar primeiro o modo de observação e testar ataques e falsos alertas antes de habilitar execução automática. A telemetria continua desligada por padrão.
