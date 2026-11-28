# Original request

Status: Source material for a proposal; not an implementation instruction accepted by a separate executor.

The user's words, preserved verbatim:

> Planeje a framework propria pro Faber ideal para trabalhar do jeito mais eficiente e de maior qualidade possivel. Melhor que BMAD e GSD estrutura de arquivos intuitivas e otimizadas para IAs e tbm para humanos poderem ver o progresso atual e oque está planejado e etc. Tipo uma roadmap bem visual e facil de entender para o usuario e coisas assim. QUERO A MELHOR FRAMEWORK POSSIVEL ESPECIFICAMENTE PRO FABER CRIADA COM BASE NO FABER COMPLETO

## Revision 2 request

The follow-up request, preserved verbatim:

> veja BMAD e GSD veja todas queixas de ambos e refaça/replaneje o Faber Flow para deixar melhor que ambas para tudo não só software tipo um framework/workflow que faça o Faber deixar até modelos "burros" como haiku, 3.8 flash, gpt-6 luna inteligentes e bons em tudo sei lá. E tbm deixar os modelos de fronteira melhores ainda para fazer os trabalhos  e não só utilizar um modelo sei lá veja como é o melhor jeito absoluto pro faber.

Interpretation: investigate broadly and redesign the workflow for the full Faber. This is planning authorization,
not a product decision to add another vendor or a claim that every online complaint can be exhaustively located.

## Revision 3 request

The next request and supplied research lead, preserved verbatim:

```text
melhore mais isso veja workflows e frameworks de outras pessoas e empresas relevantes, veja todas informações que eu ja dei e fontes no desenvolvimento do faber e etc uma pesquisa extensa mesmo e faça uma FrameWork/Workflow absoluto flexivel e otimo para tudo. Faça pesquisas na web tipo essa minha:
Best Frameworks/Workflow for claude from relevant companies
A ia do google respondeu isso:

1. Claude Code Workflows (Desenvolvimento e Engenharia)

Desenvolvidos para o agente de terminal da Anthropic, combinando arquivos de contexto e automação:
• The CLAUDE.md Operating Manual: Um arquivo de instruções na raiz do projeto que define a verdade do produto, regras de arquitetura, comandos de teste/build e restrições rígidas (ex: nunca alterar autenticação ou billing automaticamente).
• Plan-Then-Build (The /go Cycle): Usado para tarefas complexas (>1 hora). Primeiro força o Claude a entrar em Plan Mode para mapear a solução, revisar o plano com o humano e só então executar a escrita de código.
• TDD Red-Green Loop: Solicita que o Claude escreva os testes que falham primeiro (red), execute, implemente a lógica mínima para passar nos testes (green) e faça a refatoração.
• Parallel Agents via Git Worktrees: Espalha sub-agentes autônomos em worktrees isoladas do Git para lidar com partes paralelas ou auditorias de segurança (paranoid pass) sem conflitar o código principal.
• SuperClaude Framework: Uma estrutura de configuração comunitária (SuperClaude_Framework) que expande o Claude Code com personas especialistas em contexto, comandos customizados e integração com MCP (Model Context Protocol) para automação de browser e documentação.

2. Padrões de Orquestração de Agentes (Anthropic Architecture)

A Anthropic documenta três padrões fundamentais de workflow para agentes baseados em LLM que estruturam como a inteligência é aplicada:
• Sequential Workflows: Execução de tarefas em uma ordem linear e fixa, onde cada etapa passa o bastão refinado para a próxima (ex: Contract Designer -> Validator Generator -> SQL Synthesizer para APIs).
• Multi-Agent Research / Agent-Swarm: Um agente planejador divide uma tarefa complexa em subtarefas e dispara múltiplos agentes em paralelo para buscar dados ou isolar escopos simultaneamente, unificando os resultados.
• Human-in-the-Loop Guardrails: Definição estruturada de pontos de checagem obrigatórios onde a execução pausa para a validação humana antes de prosseguir com operações sensíveis (como migrações de banco ou deploy em produção).

3. Claude Cowork & Skills Workflows (Automação de Desktop e Operações)

Para fluxos de trabalho orientados a arquivos locais e interface gráfica:
• Persistência de Threads & Skills Customizadas: Uso do Claude Cowork para manter sessões contínuas conectadas a pastas locais do computador (via VM sandboxed).
• Skills-to-Routines Promocionais: No ecossistema de Claude Skills, fluxos repetitivos validados via terminal ou chat são promovidos a Routines que rodam agendadas ou em resposta a webhooks do GitHub.
Se você tiver um tipo de projeto ou área específica em mente (como desenvolvimento full-stack, automação de dados ou documentação de APIs), posso detalhar o arquivo CLAUDE.md ideal ou o passo a passo de um desses workflows.

Olhe as docs do claude e chatgpt, olhe a claude academy e claude devs no X ou blog https://claude.dev/ e tudo
https://claude.dev/blog/solving-code-review-at-anthropic/
Quero que no final vc invente o workflow framework e features pro faber trabalhar do melhor jeito possivel e coloque na roadmap.
```

The pasted search answer is a research lead, not established fact. Verify named workflows, product availability
and automation claims against current primary sources before using them as design premises.
