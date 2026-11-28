# Primeiro usuário, 2026-10-24

Uma IA que nunca viu o Faber (Codex gpt-6-luna, sem terminal nem apps) usou o app como um novato, lendo o texto na imagem da tela e vendo os
nomes dos controles, nunca o código. Janela de 720x480, idioma en, Chat com resposta gravada e sem uma IA real dentro do app, após a primeira abertura em outro app isolado sem IA.
Gerado por `node scripts/first-user.mjs`; nada aqui foi corrigido, é uma lista para ler.

As imagens ficam em `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K` (só neste computador).

## O que confundiu

- **primeira-abertura**: Escolheu “None for now” e continuou sem conectar uma IA. Na tela principal, só viu “Connect” e não entendeu se dava para usar o app sem conectar uma IA. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/primeira-abertura-6.png`)
- **projetos**: Abriu Projetos e tentou várias vezes “Add project folder”. O botão não abriu o seletor de pastas, então não conseguiu continuar nem descobrir como adicionar um projeto. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/projetos-8.png`)
- **projetos**: Abriu Projetos para descobrir para que serve. A lista vazia não explicou a função da área; encontrou a explicação só depois de abrir outra tela e voltar. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/projetos-3.png`)
- **catalogo**: Abriu os detalhes de “Systematic debugging” e rolou até o fim da página. Encontrou o que a ferramenta faz, mas não quando a IA a usa; também não viu como voltar à lista nessa tela. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/catalogo-9.png`)
- **primeira-abertura**: Abriu “Connect” e fechou a janela de conexão. Não ficou claro se fechar a janela interromperia a configuração ou levaria de volta à tela principal. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/primeira-abertura-5.png`)

## Tarefa por tarefa

### primeira-abertura: travou

Você acabou de instalar o Faber e o abriu pela primeira vez. Você ainda não conectou nenhuma IA. Passe pela primeira abertura até chegar à tela principal do app.

1. A tela diz que posso deixar a conexão para depois, então vou continuar sem conectar uma IA. → clicou em [6] "None for now" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/primeira-abertura-1.png`
2. Escolhi não conectar nenhuma IA agora; vou continuar a configuração inicial. → clicou em [8] "Continue" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/primeira-abertura-2.png`
3. Escolhi o visual escuro padrão; vou avançar para a primeira conversa. → clicou em [6] "Continue" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/primeira-abertura-3.png`
4. A tela principal já está aberta e informa que nenhuma IA está conectada; posso conectar uma assinatura agora. → clicou em [6] "Connect" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/primeira-abertura-4.png`
5. A tela oferece conectar Claude ou Codex, mas eu quero continuar sem conectar uma IA; não vejo uma opção para pular. → clicou em [6] "Close" (confuso: Não sei se fechar esta janela me leva de volta à tela principal ou interrompe a configuração.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/primeira-abertura-5.png`
6. Voltei à tela principal, mas não conectei nenhuma IA e não vejo um botão para continuar sem conexão. → travou: Na tela principal, depois de fechar a janela de conexão; só aparece o botão Connect. (confuso: Não sei se dá para usar o app sem conectar uma IA.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/primeira-abertura-6.png`

### catalogo: travou

Encontre o Catálogo, procure uma ferramenta que ajude a corrigir erros de software e abra seus detalhes. Pelo que a tela explica, descubra para que ela serve e quando a IA a usa. Depois volte à lista. Só explore e leia: não instale, remova, ative, desative ou troque ferramentas, nem abra páginas externas.

1. Não tem IA conectada; vou abrir a barra lateral pra procurar o Catálogo. → clicou em [4] "Open the sidebar" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/catalogo-1.png`
2. O Catálogo está visível na barra lateral; vou abrir a lista de ferramentas. → clicou em [8] "Catalog" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/catalogo-2.png`
3. Achei uma ferramenta que ajuda a encontrar a causa de um erro antes de mudar o código; vou abrir os detalhes dela. → clicou em [11] "Systematic debugging Finds the cause of a bug before changing any code obra/supe" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/catalogo-3.png`
4. A descrição diz que a ferramenta encontra a causa de um erro antes de mudar o código; não explica quando a IA a usa, então vou voltar à lista para continuar a exploração. → clicou em [5] "Back to the list" (confuso: Não ficou claro na tela quando a IA usa essa ferramenta.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/catalogo-4.png`
5. Voltei à lista, mas não vejo os detalhes nem quando a IA usa essa ferramenta. → clicou em [11] "Systematic debugging Finds the cause of a bug before changing any code obra/supe" (confuso: A tela só mostra a descrição curta; não encontrei quando ela é usada.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/catalogo-5.png`
6. Estou nos detalhes de Systematic debugging, mas a parte que explica quando a IA usa a ferramenta deve estar mais abaixo na página. → rolou 350 `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/catalogo-6.png`
7. Rolei a página, mas a explicação sobre quando a IA usa a ferramenta ainda não apareceu; vou descer mais. → rolou 400 `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/catalogo-7.png`
8. A tela ainda mostra os detalhes de Systematic debugging, mas não há mais conteúdo visível abaixo; vou rolar para conferir o fim da página. → rolou 400 `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/catalogo-8.png`
9. Voltei aos detalhes, mas a tela só explica o que a ferramenta faz; não encontrei quando a IA a usa. → travou: Nos detalhes de Systematic debugging, rolei até o fim e não aparece quando a IA usa essa ferramenta. Os controles disponíveis também não mostram um jeito de voltar à lista. `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/catalogo-9.png`

### conversa: conclusão declarada pela IA; precisa conferir as imagens

Comece uma conversa nova no Chat, escolha Claude Haiku no seletor de modelo e envie exatamente "In one sentence: why is the sky blue?". Acompanhe até aparecer uma resposta. É uma resposta gravada; nenhuma assinatura real está conectada dentro do app.

1. A conversa nova já está aberta e o campo de mensagem está pronto; vou escolher Claude Haiku. → escolheu "Claude Haiku" em [8] `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/conversa-1.png`
2. Claude Haiku já está escolhido; vou escrever a pergunta no campo de mensagem. → clicou em [6] "Message" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/conversa-2.png`
3. O campo de mensagem está pronto, então vou digitar a pergunta exatamente como foi pedida. → escreveu "In one sentence: why is the sky blue?" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/conversa-3.png`
4. A pergunta está pronta e Claude Haiku está selecionado; vou enviar e esperar a resposta. → clicou em [11] "Send" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/conversa-4.png`
5. A resposta já apareceu e corresponde à pergunta. → conseguiu: Escolhi Claude Haiku, enviei a pergunta e recebi uma resposta. `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/conversa-5.png`

### projetos: travou

Abra a área de Projetos e descubra para que ela serve.

1. Vejo uma conversa pronta; vou abrir o menu pra achar Projetos. → clicou em [4] "Open the sidebar" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/projetos-1.png`
2. Vejo Projetos na barra lateral; vou abrir para entender pra que serve. → clicou em [7] "Projects 0" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/projetos-2.png`
3. A área Projetos está aberta, mas só vejo uma lista vazia e não entendi para que ela serve. → clicou em [11] "Claude —" (confuso: Não há explicação nem projeto para abrir; só aparece o botão Claude na lista.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/projetos-3.png`
4. Parece que abri as configurações de contas, não a área Projetos; vou voltar para tentar encontrá-la. → clicou em [4] "← Back Esc" (confuso: A lista mostra Projetos, mas não sei como sair desta tela de configurações.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/projetos-4.png`
5. A tela diz que Projetos funciona dentro de uma pasta escolhida, então vou adicionar uma para descobrir como é. → clicou em [6] "Add project folder" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/projetos-5.png`
6. A tela explica que Projetos funciona dentro de uma pasta escolhida; vou tentar adicionar uma para ver o que acontece. → clicou em [6] "Add project folder" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/projetos-6.png`
7. A tela mostra que Projetos serve para trabalhar dentro de uma pasta escolhida; vou abrir o seletor para ver como adicionar uma. → clicou em [6] "Add project folder" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/projetos-7.png`
8. A tela diz que Projetos funciona dentro de uma pasta escolhida, mas não aparece nenhum seletor nem outro controle para continuar. → travou: Na tela vazia de Projetos; o botão “Add project folder” não abre um seletor de pastas. (confuso: Já tentei abrir “Add project folder” várias vezes, mas nada mudou.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/projetos-8.png`

### configuracoes: conclusão declarada pela IA; precisa conferir as imagens

Encontre as Configurações do app e descubra onde se muda o idioma.

1. Estou na tela Projects e não vejo as Configurações; vou abrir o menu lateral para procurar. → clicou em [4] "Open the sidebar" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/configuracoes-1.png`
2. Achei Configurações no menu lateral; vou abrir pra procurar o idioma. → clicou em [10] "Settings" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/configuracoes-2.png`
3. Estou nas configurações de contas; talvez o idioma esteja em Aparência ou Geral. → clicou em [5] "General" (confuso: Não vejo nenhuma opção de idioma nesta tela.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/configuracoes-3.png`
4. Encontrei o idioma nas configurações Gerais; vou abrir a lista pra escolher outro. → escolheu "Português (Brasil)" em [10] `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/configuracoes-4.png`
5. Escolhi Português (Brasil) na lista de idioma das configurações gerais. → conseguiu: Abri Configurações, fui em Geral e escolhi Português (Brasil) na lista Idioma. `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-WpON7K/configuracoes-5.png`
