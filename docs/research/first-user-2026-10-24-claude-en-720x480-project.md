# Primeiro usuário, 2026-10-24

Uma IA que nunca viu o Faber (Claude Haiku, sem nenhuma ferramenta) usou o app como um novato, lendo o texto na imagem da tela e vendo os
nomes dos controles, nunca o código. Janela de 720x480, idioma en, projeto Demo já preparado, com resposta gravada e sem uma IA real dentro do app.
Gerado por `node scripts/first-user.mjs`; nada aqui foi corrigido, é uma lista para ler.

As imagens ficam em `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-jjP237` (só neste computador).

## O que confundiu

- **conversa-projeto**: Confirmar que a conversa era do projeto Demo e ver a resposta final. O título da conversa mostra a pergunta e não 'Demo', então não dá para confirmar pela tela em qual projeto estou. A escolha do modelo fica lá embaixo, fora da vista. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-jjP237/conversa-projeto-9.png`)
- **conversa-projeto**: Esperar a resposta do Claude Haiku. Não ficou claro se 'The work so far · details' era a resposta ou só um resumo do trabalho em andamento. Não sabia se devia esperar ou clicar. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-jjP237/conversa-projeto-8.png`)
- **conversa-projeto**: Achar o projeto Demo. O menu do lado esquerdo mostra só Chat, Projects (1) e Groups. O Demo não aparece direto, e foi preciso entrar em Projects para achá-lo. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-jjP237/conversa-projeto-2.png`)
- **conversa-projeto**: Abrir uma conversa no projeto Demo a partir da tela inicial. Na tela inicial não havia nenhum lugar para escolher o projeto. O único botão de menu era 'Open the sidebar', e só por tentativa cheguei ao projeto. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-jjP237/conversa-projeto-1.png`)
- **conversa-projeto**: Trocar o modelo para Claude Haiku. A escolha de modelo já vinha com 'AI: choose for me', sem dizer que era preciso escolher o Claude Haiku antes de enviar. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-jjP237/conversa-projeto-4.png`)

## Tarefa por tarefa

### conversa-projeto: conclusão declarada pela IA; precisa conferir as imagens

O projeto de demonstração Demo já foi preparado para este teste. Abra uma conversa nesse projeto, escolha Claude Haiku no seletor de modelo e envie exatamente "In one sentence: why is the sky blue?". Acompanhe até aparecer uma resposta. É uma resposta gravada: não avalie edição de arquivos nem criação do projeto.

1. Vejo a tela inicial do Faber, sem conversa aberta, e preciso primeiro abrir uma conversa no projeto Demo. → clicou em [4] "Open the sidebar" (confuso: Não vejo nenhum seletor de projeto na tela; o único botão de menu é o [4] 'Open the sidebar', então talvez o projeto Demo esteja lá.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-jjP237/conversa-projeto-1.png`
2. A tela mostra o Faber com a barra lateral aberta, e o botão de nova conversa está visível no topo. → clicou em [7] "Projects 1" (confuso: Não vejo o projeto Demo na tela; a lista lateral mostra só Chat, Projects (1) e Groups vazio, então preciso passar por Projects.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-jjP237/conversa-projeto-2.png`
3. A tela mostra Projects com 'Demo' listado na barra lateral, então o próximo passo é abrir esse projeto. → clicou em [10] "Demo" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-jjP237/conversa-projeto-3.png`
4. A conversa do projeto Demo já está aberta e o seletor de modelo mostra 'AI: choose for me', então preciso trocar para Claude Haiku antes de enviar a pergunta. → escolheu "Claude Haiku" em [8] `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-jjP237/conversa-projeto-4.png`
5. O seletor já mostra Claude Haiku e o campo de mensagem está vazio, então falta só digitar a pergunta. → clicou em [6] "Message" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-jjP237/conversa-projeto-5.png`
6. A conversa do Demo está aberta com Claude Haiku selecionado e o campo de mensagem vazio, então falta digitar a pergunta. → escreveu "In one sentence: why is the sky blue?" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-jjP237/conversa-projeto-6.png`
7. A pergunta já está no campo de mensagem e o seletor mostra Claude Haiku, então o próximo passo é enviar. → clicou em [11] "Send" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-jjP237/conversa-projeto-7.png`
8. A pergunta já foi enviada e a resposta do Claude Haiku 5.5 ainda aparece em andamento, com o cursor piscando e '1 running'. → aguardou (confuso: Não está claro se 'The work so far · details' é a resposta ou só um resumo do trabalho em andamento.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-jjP237/conversa-projeto-8.png`
9. A resposta do Claude Haiku já apareceu na tela, com 'Done · updated just now' e 'Nothing running' no topo, então a tarefa terminou. → conseguiu: Abri o Demo pela barra lateral, escolhi Claude Haiku, enviei 'In one sentence: why is the sky blue?' e a resposta apareceu completa: a luz do sol contém todas as cores e o ar espalha mais o azul (espalhamento de Rayleigh), então a luz azul chega aos olhos de todas as direções do céu. O status final é 'Done' e 'Nothing running'. (confuso: O cabeçalho mostra 'In one sentence: why is t...' e não 'Demo', então não consigo confirmar pela tela que é o projeto Demo; o seletor também fica abaixo da rolagem.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-jjP237/conversa-projeto-9.png`
