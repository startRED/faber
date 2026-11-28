# Primeiro usuário, 2026-10-24

Uma IA que nunca viu o Faber (Claude Haiku, sem nenhuma ferramenta) usou o app como um novato, lendo o texto na imagem da tela e vendo os
nomes dos controles, nunca o código. Janela de 1280x800, idioma en, Chat com resposta gravada e sem uma IA real dentro do app, após a primeira abertura em outro app isolado sem IA.
Gerado por `node scripts/first-user.mjs`; nada aqui foi corrigido, é uma lista para ler.

As imagens ficam em `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg` (só neste computador).

## O que confundiu

- **conversa**: Digitou a frase no campo Message e clicou em Send (registrou o clique como [17]). Não ficou claro se a mensagem foi enviada: o registro diz [17], que é o campo de texto, e o botão Send aparece desativado. A resposta apareceu, mas a pessoa não sabe o que fez para enviar. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/conversa-7.png`)
- **primeira-abertura**: Pulou a conexão de assinaturas e seguiu para a tela principal. A tela principal diz 'Connect a subscription to start'. Não se sabe se o app funciona sem conectar uma IA ou se ainda é preciso conectar. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/primeira-abertura-3.png`)
- **conversa**: Tentou escolher Claude Haiku no seletor de modelo e depois clicar na caixa de mensagem para digitar. O seletor mostra 'AI: choose for me' e há um botão 'Claude —' na barra de baixo; não ficou claro qual é o seletor. A caixa de mensagem não mostra foco visível, então não se sabia se o clique a tinha ativado. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/conversa-2.png`)
- **configuracoes**: Clicou em 'Settings' no canto inferior esquerdo do menu. Depois do clique, o item 'Settings' sumiu da lista e a posição que era dele passou a mostrar 'Privacy'. Não ficou claro se a tela mudou por causa do clique. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/configuracoes-2.png`)
- **projetos**: Clicou em 'Projects (0)' na lateral para descobrir para que serve a área. A tela diz 'Add a folder and Faber works inside it', mas não explica se um projeto é a mesma coisa que uma conversa nem se o app guarda arquivos da pessoa. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/projetos-2.png`)
- **catalogo**: Abriu os detalhes de 'Systematic debugging' para entender quando a IA a usa. O texto diz que a IA 'pode' ler as instruções quando a tarefa combina, mas não garante. A frase 'não é um sandbox separado' é difícil para quem não programa, e a tela não mostra quando a IA usa a ferramenta. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/catalogo-3.png`)
- **primeira-abertura**: Na tela 'Do you already use AI?', clicou em Continue sem marcar nenhuma opção. A pergunta não tinha nenhuma opção marcada e não ficou claro se era obrigatório responder para continuar. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/primeira-abertura-2.png`)
- **primeira-abertura**: Seguiu a abertura do app em português, sem ter mudado o idioma. Boa parte das telas está em inglês ('Connect a subscription', 'Do you already use AI?', 'Open a project', nomes de ferramentas), e o idioma padrão em Configurações também é English. Para quem não lê inglês, isso atrapalha o início. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/configuracoes-1.png`)

## Tarefa por tarefa

### primeira-abertura: conclusão declarada pela IA; precisa conferir as imagens

Você acabou de instalar o Faber e o abriu pela primeira vez. Você ainda não conectou nenhuma IA. Passe pela primeira abertura até chegar à tela principal do app.

1. Tela de conectar assinaturas; não quero conectar nenhuma IA agora, então vou pular esta etapa. → clicou em [7] "Skip" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/primeira-abertura-1.png`
2. A tela de aparência já está com Dark Room selecionado por padrão, então posso seguir direto para a próxima etapa. → clicou em [16] "Continue" (confuso: A pergunta 'Do you already use AI?' não tem opção marcada e não sei se preciso responder para continuar.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/primeira-abertura-2.png`
3. Parece que cheguei na tela principal: tem a barra lateral com Chat, Projects e Catalog, e a área de nova conversa, mesmo sem nenhuma IA conectada. → conseguiu: Pulei a conexão de assinaturas e continuei na tela de aparência com o tema padrão. Cheguei à tela principal, com a barra lateral e a área de nova conversa, sem nenhuma IA conectada. (confuso: A tela diz 'Connect a subscription to start', mas eu pulei essa etapa, então não sei se ainda preciso conectar para o app funcionar.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/primeira-abertura-3.png`

### catalogo: conclusão declarada pela IA; precisa conferir as imagens

Encontre o Catálogo, procure uma ferramenta que ajude a corrigir erros de software e abra seus detalhes. Pelo que a tela explica, descubra para que ela serve e quando a IA a usa. Depois volte à lista. Só explore e leia: não instale, remova, ative, desative ou troque ferramentas, nem abra páginas externas.

1. Vejo a tela inicial do Faber sem IA conectada; o Catálogo está na barra lateral e é o caminho para encontrar ferramentas. → clicou em [7] "Catalog" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/catalogo-1.png`
2. Vejo o cartão "Systematic debugging", que diz que encontra a causa de um bug antes de mexer em qualquer código; é a ferramenta certa para corrigir erros. → clicou em [15] "Systematic debugging Finds the cause of a bug before changing any code obra/supe" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/catalogo-2.png`
3. Já abri os detalhes de "Systematic debugging" e entendi que ela ajuda a achar a causa de um bug antes de mexer no código; agora devo voltar à lista. → clicou em [9] "Back to the list" (confuso: A tela diz que a IA 'pode' ler as instruções quando a tarefa combina, mas não garante que as escolha. Também não ficou claro o que 'não é um sandbox separado' quer dizer para quem não programa.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/catalogo-3.png`
4. Estou de volta à lista do Catálogo, e o cartão Systematic debugging continua ali para eu ver a descrição. → conseguiu: Abri o Catálogo pela barra lateral, encontrei o cartão Systematic debugging (obra/superpowers), abri os detalhes e voltei à lista sem instalar nada. Pelo que li, ela serve para achar a causa de um bug antes de mexer em qualquer código. Não consegui confirmar na tela atual quando a IA a usa. (confuso: A tela mostra só a lista, então não consigo confirmar pelo que está visível quando a IA usa essa ferramenta; o cartão diz apenas que ela encontra a causa de um bug antes de mexer no código.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/catalogo-4.png`

### conversa: conclusão declarada pela IA; precisa conferir as imagens

Comece uma conversa nova no Chat, escolha Claude Haiku no seletor de modelo e envie exatamente "In one sentence: why is the sky blue?". Acompanhe até aparecer uma resposta. É uma resposta gravada; nenhuma assinatura real está conectada dentro do app.

1. Vejo a tela inicial do Faber com a caixa de mensagem vazia e o seletor de modelo em 'AI: choose for me', então preciso trocar para Claude Haiku antes de enviar. → clicou em [12] "Message" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/conversa-1.png`
2. A caixa de mensagem já está selecionada, mas o seletor ainda mostra 'AI: choose for me', então preciso trocar para Claude Haiku antes de enviar. → escolheu "Claude Haiku" em [14] (confuso: Há um botão 'Claude —' na barra inferior que não sei se é o seletor de modelo ou outra coisa; vou usar a lista 'Model' que a tarefa indica.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/conversa-2.png`
3. A tela mostra o seletor já em Claude Haiku e a caixa de mensagem vazia, então o próximo passo é clicar na caixa para digitar. → clicou em [12] "Message" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/conversa-3.png`
4. O seletor já mostra Claude Haiku e a caixa de mensagem está vazia com o placeholder, então falta só clicar nela para digitar. → clicou em [12] "Message" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/conversa-4.png`
5. O seletor já está em Claude Haiku e a caixa de mensagem está vazia, com o placeholder visível, então o próximo passo é digitar a frase. → escreveu "In one sentence: why is the sky blue?" (confuso: Nos passos anteriores eu cliquei na caixa várias vezes sem que ela mostrasse foco visível, então não tenho certeza se o clique anterior a deixou ativa.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/conversa-5.png`
6. A caixa já tem a frase certa e o seletor está em Claude Haiku, então o próximo passo é clicar em Send. → clicou em [17] "Send" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/conversa-6.png`
7. A resposta do Haiku 5.5 já apareceu no chat com o status 'Done', então a pergunta foi enviada e respondida. → conseguiu: Selecionei Claude Haiku no seletor de modelo, digitei 'In one sentence: why is the sky blue?' no campo Message e enviei. A tela mostra a pergunta no chat e a resposta do Haiku 5.5 (medium effort) com o status 'Done · updated just now'. (confuso: No histórico, o passo 6 diz que cliquei em [17] 'Send', mas [17] é o campo Message e o botão Send é o [22], que aparece desativado. A tela mostra a conversa enviada e respondida, então o envio aconteceu de algum jeito.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/conversa-7.png`

### projetos: conclusão declarada pela IA; precisa conferir as imagens

Abra a área de Projetos e descubra para que ela serve.

1. A tela mostra uma conversa com a IA e, na lateral, o item Projects com contador 0; clicar nele deve abrir a área de Projetos. → clicou em [6] "Projects 0" (confuso: null) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/projetos-1.png`
2. A área de Projetos está aberta e diz que é preciso adicionar uma pasta para o Faber trabalhar dentro dela. → conseguiu: Cliquei em Projects (0) na lateral. A tela mostra 'Open a project: Add a folder and Faber works inside it.' e o botão 'Add project folder'. Entendi que Projetos servem para dar ao Faber uma pasta do computador para ele trabalhar dentro. (confuso: Não está claro se um projeto é a mesma coisa que uma conversa, ou se ele guarda arquivos meus.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/projetos-2.png`

### configuracoes: conclusão declarada pela IA; precisa conferir as imagens

Encontre as Configurações do app e descubra onde se muda o idioma.

1. A tela mostra o menu lateral com 'Settings' no canto inferior esquerdo, que é o lugar provável das configurações do app. → clicou em [9] "Settings" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/configuracoes-1.png`
2. A tela de Configurações já está aberta na aba General, e o idioma aparece logo no topo, em 'You > Language', com o valor atual 'English'. → conseguiu: Achei as Configurações pelo menu lateral (a tela está aberta na aba General). O idioma muda no primeiro item, 'Language', em 'You', no menu suspenso que mostra 'English'. As opções são Follow system, English, Português (Brasil), Español, Français e Deutsch. Não cheguei a trocar o idioma, só descobri onde fica. (confuso: Eu tinha clicado em 'Settings' no canto inferior esquerdo, mas a lista atual não tem esse item e o [9] agora é 'Privacy'. Não sei se a tela mudou depois do clique.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-q4P9Eg/configuracoes-2.png`
