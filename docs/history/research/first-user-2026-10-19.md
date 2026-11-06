# Primeiro usuário, 2026-10-19

Uma IA que nunca viu o Faber (Claude Haiku, sem nenhuma ferramenta) usou o app como um novato, vendo só a tela: a imagem, o texto visível e os
nomes dos controles, nunca o código. Janela de 720x480, idioma pt-BR, primeira abertura sem nenhuma IA
conectada. Gerado por `node scripts/first-user.mjs`; nada aqui foi corrigido, é uma lista para ler.

As imagens ficam em `target/faber-test/first-user/2026-10-19` (só neste computador).

## O que confundiu

- **conversa**: Tentou começar uma conversa, clicando em Conectar, Fechar, no ícone do menu e em Nova conversa. A tela só mostra 'Nenhuma IA conectada' e não há onde escrever a pergunta. Não ficou claro se é preciso conectar uma IA antes de conversar. (`target/faber-test/first-user/2026-10-19/conversa-5.png`)
- **projetos**: Tentou abrir a área de Projetos e entender para que ela serve. A área está vazia e só pede para conectar uma assinatura. Nenhuma frase explica o que é um projeto ou o que se faz nele. (`target/faber-test/first-user/2026-10-19/projetos-6.png`)
- **primeira-abertura**: Escolheu 'Nenhuma por enquanto' e cliquei em Continuar até chegar à tela principal. A tela principal diz 'Nenhuma IA conectada' e só tem o botão Conectar. Não ficou claro se ainda falta alguma configuração ou se já dá para começar a usar. (`target/faber-test/first-user/2026-10-19/primeira-abertura-4.png`)
- **projetos**: Procurou o botão Projetos na tela principal. Projetos não aparece na tela principal. Só foi encontrado depois de abrir a barra lateral, e não ficou claro que ele fica dentro desse menu. (`target/faber-test/first-user/2026-10-19/projetos-1.png`)
- **configuracoes**: Clicou no ícone de três linhas no canto superior esquerdo para procurar Configurações. Em vez de um menu com Configurações, abriu o painel 'Rodando agora'. Nada na tela falava em Configurações ou idioma. (`target/faber-test/first-user/2026-10-19/configuracoes-2.png`)
- **conversa**: Procurou uma forma de começar uma conversa pela barra lateral. 'Conversa nova' aparece só como texto, sem indicar que pode ser clicado. Não ficou claro se funciona sem uma IA conectada. (`target/faber-test/first-user/2026-10-19/conversa-3.png`)
- **primeira-abertura**: Na escolha de aparência, deixou a Sala escura e clicou em Continuar. A tela pedia para ajustar tamanho do texto, espaçamento e movimento, mas não ficou claro se era preciso fazer isso antes de continuar. (`target/faber-test/first-user/2026-10-19/primeira-abertura-3.png`)
- **configuracoes**: Abriu a barra lateral pelo ícone de três linhas e tentou achar Configurações. Configurações só apareceu depois de abrir a barra lateral. Não ficou claro se a barra ficou aberta por causa do clique anterior. (`target/faber-test/first-user/2026-10-19/configuracoes-4.png`)

## Tamanhos de tela (pedido do Erick)

Fotos das telas principais em 2560x1440, 1920x1080, 1600x900, 1536x864, 1440x900, 1366x768, 1280x800, 1280x720,
1024x768 e 720x480 (`target/faber-test/sizes/`, só neste computador). De 1024 a 2560 de largura tudo tem o mesmo
tamanho em pixels: texto 14 px, barra lateral 248 px, caixa de escrever 640 px, título 86 px. Em 1280x720 a tela fica
cheia e equilibrada; em 2560x1440 (sem a escala do Windows) tudo fica pequeno e encostado à esquerda, com mais da metade
da tela vazia. A escala do Windows (125%, 150%) compensa em parte, mas quem usa 100% num monitor grande vê o app miúdo.
Só em 720x480 o título encolhe (65 px). Nada passou da largura da janela em nenhum tamanho.

## Tarefa por tarefa

### primeira-abertura: conseguiu

Você acabou de instalar o Faber e o abriu pela primeira vez. Você ainda não conectou nenhuma IA. Passe pela primeira abertura até chegar à tela principal do app.

1. A tela pede para conectar assinaturas de IA, mas ainda não tenho nenhuma, então escolho "Nenhuma por enquanto". → clicou em [6] "Nenhuma por enquanto" `target/faber-test/first-user/2026-10-19/primeira-abertura-1.png`
2. A opção "Nenhuma por enquanto" já está marcada, então posso seguir em frente com Continuar. → clicou em [8] "Continuar" `target/faber-test/first-user/2026-10-19/primeira-abertura-2.png`
3. Agora pede para escolher a aparência; deixo a Sala escura padrão e posso seguir em frente. → clicou em [6] "Continuar" (confuso: Não sei se preciso ajustar o tamanho do texto, o espaço e o movimento antes de continuar, mas posso mudar tudo depois em Configurações.) `target/faber-test/first-user/2026-10-19/primeira-abertura-3.png`
4. Parece que já estou na tela principal do Faber: tem 'Conversa nova', 'Nada rodando' e a área de conversa vazia, só falta conectar uma IA. → conseguiu: Na primeira abertura, escolhi 'Nenhuma por enquanto' para as assinaturas de IA, cliquei em 'Continuar' e depois em 'Continuar' de novo na escolha de aparência, mantendo a Sala escura. Cheguei à tela principal com 'Conversa nova' e 'Nada rodando', sem nenhuma IA conectada. (confuso: A tela diz 'Nenhuma IA conectada' e só mostra um botão 'Conectar', então não sei se ainda falta algum passo de configuração ou se posso começar a conversar assim.) `target/faber-test/first-user/2026-10-19/primeira-abertura-4.png`

### conversa: travou

Comece uma conversa nova e mande uma pergunta simples, por exemplo "o que você sabe fazer?".

1. Vejo o app Faber sem IA conectada, com um botão 'Conectar' e o botão 'Conversa nova' no topo, mas ainda não há campo de texto visível para perguntar nada. → clicou em [6] "Conectar" (confuso: A tela diz 'Nenhuma IA conectada' e pede para conectar uma assinatura antes de começar, então não parece dar para mandar a pergunta direto; não sei se preciso conectar primeiro.) `target/faber-test/first-user/2026-10-19/conversa-1.png`
2. Vejo uma janela de conexão de assinaturas, com opções Claude, Codex e 'Nenhuma por enquanto', e um botão Fechar; não há campo de conversa. → clicou em [7] "Fechar" (confuso: A tela pede para conectar assinaturas, mas não sei se preciso conectar uma antes de conversar ou se posso só fechar essa janela para chegar ao chat.) `target/faber-test/first-user/2026-10-19/conversa-2.png`
3. A tela ainda mostra só 'Nenhuma IA conectada' e não tem campo para escrever, então vou abrir a barra lateral para ver se existe uma conversa nova por lá. → clicou em [4] "Abrir a barra lateral" (confuso: Não vejo nenhum campo de texto para perguntar nada, e o botão 'Conversa nova' aparece como texto sem número, então não sei se ele é clicável.) `target/faber-test/first-user/2026-10-19/conversa-3.png`
4. A tela ainda mostra 'Nenhuma IA conectada' e não há campo de texto, então vou tentar o botão 'Nova conversa' da barra lateral. → clicou em [4] "Nova conversa Ctrl N" (confuso: Não há campo para escrever a pergunta enquanto nenhuma IA estiver conectada, e não sei se 'Nova conversa' funciona sem uma conexão.) `target/faber-test/first-user/2026-10-19/conversa-4.png`
5. Vejo que a tela ainda mostra só 'Nenhuma IA conectada' e não há campo de texto, então o caminho é conectar uma assinatura de fato. → travou: A tela de 'Nenhuma IA conectada' não mostra campo de texto. O único caminho visível é conectar uma assinatura (Claude ou Codex), e a janela de conexão fechou sem conectar nada. Para começar a conversa é preciso conectar uma IA, o que exige uma assinatura ou conta que eu não tenho. (confuso: Já cliquei em Conectar, Fechar, Abrir a barra lateral e Nova conversa, e nenhum deles mostrou campo para escrever; não sei se preciso de uma IA conectada para o campo aparecer.) `target/faber-test/first-user/2026-10-19/conversa-5.png`

### projetos: travou

Abra a área de Projetos e descubra para que ela serve.

1. Não vejo a área de Projetos na tela; o ícone de menu no canto superior esquerdo provavelmente abre uma barra lateral com as opções. → clicou em [4] "Abrir a barra lateral" (confuso: Não aparece nenhum botão chamado Projetos. Não sei se ela fica dentro do menu ou em outro lugar.) `target/faber-test/first-user/2026-10-19/projetos-1.png`
2. Vejo a barra lateral aberta com 'Projetos' e um contador 0, então vou abrir essa área para ver o que ela mostra. → clicou em [6] "Projetos 0" (confuso: Não sei ainda o que significa o número 0 ao lado de Projetos; pode ser a quantidade de projetos criados.) `target/faber-test/first-user/2026-10-19/projetos-2.png`
3. A tela mostra só 'Nada rodando' e 'Nenhuma IA conectada', então a área de Projetos parece vazia e depende de conectar uma IA primeiro. → clicou em [6] "Conectar" (confuso: Não está claro para que serve a área de Projetos; ela só diz que não há nada rodando e que é preciso conectar uma assinatura, sem explicar o que um projeto é.) `target/faber-test/first-user/2026-10-19/projetos-3.png`
4. A tela pede para conectar assinaturas de IA, mas eu quero ver o que a área de Projetos mostra, então vou fechar este diálogo sem conectar nada. → clicou em [7] "Fechar" (confuso: Não está claro se a área de Projetos exige uma IA conectada para funcionar ou se só mostra a lista vazia.) `target/faber-test/first-user/2026-10-19/projetos-4.png`
5. A tela de Projetos está vazia e pede para conectar uma assinatura de IA antes de começar, mas ainda não entendo para que serve a área. → clicou em [5] "Nada rodando" (confuso: A tela não explica o que é um projeto nem o que faço nele, e o botão do menu que eu usei antes não aparece mais na lista de controles.) `target/faber-test/first-user/2026-10-19/projetos-5.png`
6. A área de Projetos está praticamente vazia e só pede para conectar uma assinatura, então ainda não dá para ver para que ela serve. → travou: Estou na área Projetos, mas ela está vazia ('Nenhuma IA conectada', 'Conecte uma assinatura para começar'). Não consigo descobrir sua finalidade sem conectar uma IA, e não conectei por decisão nesta tarefa. Preciso de orientação: devo conectar uma assinatura para continuar? (confuso: Não há nenhuma descrição do que um projeto é ou faz; a única forma de descobrir parece ser conectar uma IA, o que decidi não fazer nesta tarefa.) `target/faber-test/first-user/2026-10-19/projetos-6.png`

### configuracoes: conseguiu

Encontre as Configurações do app e descubra onde se muda o idioma.

1. Não vejo nada com a palavra Configurações; o ícone de três linhas no canto superior esquerdo costuma abrir um menu lateral com essas opções. → clicou em [4] "Abrir a barra lateral" (confuso: Não aparece nenhum botão de Configurações na tela principal, então não sei se ela fica dentro de algum menu.) `target/faber-test/first-user/2026-10-19/configuracoes-1.png`
2. Cliquei no ícone de três linhas e a tela não mostrou um menu com Configurações; só apareceu o painel da direita, 'Rodando agora'. → clicou em [7] "Fechar a lista do que está rodando" (confuso: Não sei se a barra lateral abriu ou se o painel da direita está tapando o menu. Nenhum texto da tela fala em Configurações ou idioma.) `target/faber-test/first-user/2026-10-19/configuracoes-2.png`
3. Ainda não vejo Configurações; o ícone de três linhas é o lugar mais provável para um menu, então vou tentá-lo de novo. → clicou em [4] "Abrir a barra lateral" (confuso: O ícone de três linhas não mostrou um menu com Configurações na primeira vez, e o painel 'Rodando agora' apareceu no lugar dele.) `target/faber-test/first-user/2026-10-19/configuracoes-3.png`
4. A barra lateral está aberta e o item 'Configurações' aparece agora na lista, que é o lugar provável para mudar o idioma. → clicou em [8] "Configurações" (confuso: Antes eu não via 'Configurações' e o ícone de três linhas abriu outro painel; agora o item está visível, então não sei se a barra lateral ficou aberta por causa do clique anterior.) `target/faber-test/first-user/2026-10-19/configuracoes-4.png`
5. A tela Geral de Configurações mostra o campo Idioma, com Português (Brasil) selecionado, então o lugar para mudar o idioma está encontrado. → conseguiu: Abri a barra lateral pelo ícone de três linhas, cliquei em Configurações e cheguei à tela Geral. O idioma muda no campo Idioma, na seção Você, que hoje mostra Português (Brasil). (confuso: No começo não apareceu nenhum item chamado Configurações; ele só surgiu depois de abrir a barra lateral pelo ícone de três linhas.) `target/faber-test/first-user/2026-10-19/configuracoes-5.png`
