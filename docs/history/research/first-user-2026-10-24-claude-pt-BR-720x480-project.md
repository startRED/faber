# Primeiro usuário, 2026-10-24

Uma IA que nunca viu o Faber (Claude Haiku, sem nenhuma ferramenta) usou o app como um novato, lendo o texto na imagem da tela e vendo os
nomes dos controles, nunca o código. Janela de 720x480, idioma pt-BR, projeto Demo já preparado, com resposta gravada e sem uma IA real dentro do app.
Gerado por `node scripts/first-user.mjs`; nada aqui foi corrigido, é uma lista para ler.

As imagens ficam em `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-HHhpG4` (só neste computador).

## O que confundiu

- **conversa-projeto**: Abrir o projeto Demo pela barra lateral, escolher Claude Haiku e enviar a pergunta 'In one sentence: why is the sky blue?'. Na tela inicial não aparecia nenhum projeto para escolher; só 'Conversa nova' e o campo de mensagem, então não havia onde começar sem adivinhar o botão do menu. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-HHhpG4/conversa-projeto-1.png`)
- **conversa-projeto**: Enviar a pergunta depois de escolher o modelo. O modelo aparecia como Claude Haiku na imagem, mas a lista de controles dizia 'IA: escolher por mim'. Não dava para saber qual modelo ia responder. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-HHhpG4/conversa-projeto-5.png`)
- **conversa-projeto**: Digitar a pergunta na caixa de mensagem. O texto dentro da caixa dizia 'Encontre o teste que falha e corrija', que parece uma tarefa de programação e não um lugar para perguntar. Também não ficou claro o que é 'Acesso total'. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-HHhpG4/conversa-projeto-6.png`)
- **conversa-projeto**: Esperar a resposta depois de enviar. A tela mostrava 'Haiku 5.5 · esforço médio', um cursor piscando e 'O trabalho até aqui · detalhes', mas nenhum texto de resposta. Não deu para saber se a pergunta tinha sido enviada. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-HHhpG4/conversa-projeto-8.png`)
- **conversa-projeto**: Ler a resposta final. A parte de cima da resposta ficou cortada pela rolagem, então não dava para ler a frase inteira. (`E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-HHhpG4/conversa-projeto-9.png`)

## Tarefa por tarefa

### conversa-projeto: conclusão declarada pela IA; precisa conferir as imagens

O projeto de demonstração Demo já foi preparado para este teste. Abra uma conversa nesse projeto, escolha Claude Haiku no seletor de modelo e envie exatamente "In one sentence: why is the sky blue?". Acompanhe até aparecer uma resposta. É uma resposta gravada: não avalie edição de arquivos nem criação do projeto.

1. Vejo a tela inicial do Faber, mas o projeto Demo não aparece aqui; o botão de menu no canto superior esquerdo deve abrir a barra lateral com os projetos. → clicou em [4] "Abrir a barra lateral" (confuso: Não encontro na tela onde escolher o projeto Demo; só vejo 'Conversa nova' e o campo de mensagem.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-HHhpG4/conversa-projeto-1.png`
2. A barra lateral abriu e mostra Projetos com um contador 1, então o projeto Demo deve estar lá. → clicou em [6] "Projetos 1" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-HHhpG4/conversa-projeto-2.png`
3. A barra lateral abriu e mostra o projeto Demo logo abaixo de Projetos, então vou abri-lo. → clicou em [9] "Demo" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-HHhpG4/conversa-projeto-3.png`
4. A tela já mostra o projeto Demo aberto, com a caixa de mensagem vazia e o modelo ainda em "IA: escolher por mim". → escolheu "Claude Haiku" em [8] `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-HHhpG4/conversa-projeto-4.png`
5. A tela já mostra o projeto Demo aberto, com a caixa de mensagem e o seletor de modelo, então falta só digitar a pergunta. → clicou em [6] "Mensagem" (confuso: O seletor de modelo mostrado na imagem aparece como Claude Haiku, mas a lista de controles diz que o modelo ainda está em 'IA: escolher por mim', então não tenho certeza de qual está valendo.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-HHhpG4/conversa-projeto-5.png`
6. A caixa de mensagem já está selecionada e o modelo é Claude Haiku, então falta só digitar a pergunta. → escreveu "In one sentence: why is the sky blue?" (confuso: O texto de dentro da caixa ('Encontre o teste que falha e corrija') parece uma tarefa de programação, não um espaço para minha pergunta. Também não sei o que significa 'Acesso total'.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-HHhpG4/conversa-projeto-6.png`
7. A pergunta já está digitada no campo e o seletor mostra Claude Haiku, então o próximo passo é clicar em Enviar. → clicou em [11] "Enviar" `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-HHhpG4/conversa-projeto-7.png`
8. A conversa já está aberta com 'Claude Haiku' e a pergunta enviada; a tela mostra a resposta em preparação, então preciso esperar. → aguardou (confuso: A tela mostra 'Haiku 5.5 · esforço médio' com um cursor piscando e 'O trabalho até aqui · detalhes', sem texto de resposta ainda; não sei se a pergunta foi enviada de fato ou se é só o estado de preparação.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-HHhpG4/conversa-projeto-8.png`
9. A resposta já apareceu na conversa (fala de espalhamento de Rayleigh e da luz azul), e a tela mostra 'Pronto · atualizado agora há pouco'. → conseguiu: Abri o projeto Demo pela barra lateral, escolhi Claude Haiku, enviei 'In one sentence: why is the sky blue?' e esperei. A resposta apareceu na conversa, com a explicação do espalhamento de Rayleigh, e o app mostra 'Pronto'. (confuso: A parte de cima da resposta está cortada pela rolagem, então não consigo ler a frase inteira.) `E:/Documentos/ProjetosIA/faber/target/build-3/faber-test/first-user/2026-10-24-HHhpG4/conversa-projeto-9.png`
