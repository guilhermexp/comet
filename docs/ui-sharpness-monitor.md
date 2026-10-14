# Monitor de nitidez do Zeron

O monitor observa a janela do Zeron já aberta, captura PNGs sem perdas e registra mudanças nos pixels. Serve para investigar texto que parece perder nitidez durante o uso, sem mudar o tema, o vidro ou o renderer.

## Rodar durante o desenvolvimento

Com o Zeron já aberto, execute em outro terminal, na raiz do checkout:

```bash
python3 scripts/ui-sharpness-monitor.py
```

A coleta padrão dura cinco minutos, com intervalo alvo de um segundo. A frequência efetiva depende do custo de captura e análise; cada amostra registra seu horário real. Use o app normalmente. Quando perceber o desfoque, volte ao terminal e pressione **Enter** para marcar o incidente. **Ctrl+C** encerra a coleta e finaliza o relatório. O monitor imprime a pasta da rodada e os eventos ao vivo.

Ele não inicia outra cópia do app nem encerra a que está aberta. Pode observar o app de desenvolvimento ou a instalação existente. Para uma coleta curta:

```bash
python3 scripts/ui-sharpness-monitor.py --duration 30
```

Dependências: Python 3, NumPy e Pillow. Se não estiverem disponíveis no Python escolhido, use um ambiente local explícito:

```bash
python3 -m venv .tmp/ui-monitor-venv
.tmp/ui-monitor-venv/bin/python -m pip install numpy Pillow
.tmp/ui-monitor-venv/bin/python scripts/ui-sharpness-monitor.py
```

O macOS precisa permitir gravação de tela para o processo que executa a captura. O monitor verifica a permissão e explica a falha; não concede permissões nem altera configurações automaticamente.

## Evidência produzida

Cada rodada tem pasta própria e privada, com:

- `report.html`: resumo local com imagens dos incidentes.
- `report.json`: contagens, motivo do encerramento e limitações.
- `observations.jsonl`: uma observação por amostra, com horário e metadados.
- `events.jsonl`: candidatos, mudanças de referência e marcas manuais.
- PNGs de referência e imagens antes/depois dos incidentes, dentro do orçamento de retenção.

Abra o HTML local para inspecionar as letras nos quadros antes e depois. Nenhum arquivo é enviado à rede. Capturas podem conter conteúdo privado do Chat; mantenha a pasta local ao compartilhar apenas os resultados necessários.

O padrão de retenção é 256 MiB, com piso de 1 GiB livre em disco. `--out` aceita uma pasta nova ou vazia; uma rodada existente nunca é sobrescrita. O relatório registra falhas de captura, perda da janela e limites de retenção, em vez de apresentá-los como estabilidade.

## Interpretar o resultado

| Classificação | O que foi observado |
|---|---|
| `unchanged` | Não houve mudança relevante segundo a tolerância do detector. O campo `pixel_equal` distingue igualdade exata de uma pequena diferença tolerada. |
| `contrast` | A intensidade/contraste mudou, preservando os contornos. Isso pode reduzir a legibilidade sem blur espacial. |
| `blur_candidate` | Um modelo de suavização espacial explica a mudança melhor que contraste sozinho. Inspecione as imagens; é candidato, não diagnóstico da causa. |
| `content_change` | O conteúdo mudou ou se deslocou; a comparação não sustenta atribuir isso a blur. |
| `geometry_change` / `rebaseline` | Tamanho ou referência mudou; a ocorrência fica registrada separadamente. |
| `unassessable` | A região não oferece contornos suficientes para avaliar nitidez. |

A comparação é **relativa à referência**. Se a referência já estava desfocada e não muda, o monitor não pode provar que está nítida. Uma marca manual preserva evidência mesmo quando o detector não encontra variação. Uma amostra por segundo também pode perder eventos breves.

O monitor mede a saída nativa capturada pelo macOS. Ele não lê o framebuffer Metal anterior à composição, não conta repaints e não identifica sozinho qual função do renderer causou um candidato. A razão entre pixels do PNG e pontos da janela é uma observação da captura, não uma leitura de `contentsScale` interno.

## Regiões e reprocessamento

Sem seleção, o monitor amostra até oito regiões com contornos distribuídas pela janela, limitando o custo da análise. Isso não cobre todos os pixels: as regiões escolhidas ficam registradas. Cada região mantém sua própria referência, para uma mensagem nova não invalidar a comparação de outro painel. Para acompanhar um texto estático específico, use `--roi x,y,w,h` em **pixels nativos da captura**, com origem no canto superior esquerdo da janela. A opção pode ser repetida:

```bash
python3 scripts/ui-sharpness-monitor.py --duration 60 --roi 2830,100,900,450
```

As coordenadas acima são apenas um exemplo para uma janela de 3840 pixels de largura; confira as dimensões da sua captura. Se houver mais de uma janela Zeron elegível, a descoberta exige `--window-id`, em vez de escolher uma silenciosamente.

Para analisar PNGs já existentes, use uma pasta contendo apenas a sequência desejada, em ordem de nome:

```bash
python3 scripts/ui-sharpness-monitor.py --analyze /tmp/zeron-pngs --out .tmp/monitor-replay
```

O modo offline não acessa o app nem APIs de captura. Arquivos JPEG renomeados para `.png` são rejeitados. Para testar uma marca manual na segunda imagem, acrescente `--mark-at 1`; o índice começa em zero.

## Verificação do monitor

```bash
python3 -m unittest discover -s scripts/tests -p 'test_ui_sharpness*.py' -v
```

Os testes distinguem blur conhecido, contraste afim, movimento, alteração de conteúdo, geometria, pixels estáveis e ruído de quantização. Os testes da CLI percorrem PNGs reais, marcação, retenção, falhas e relatório. A comprovação nativa observa somente uma janela já aberta, conforme o contrato de QA em `scripts/AGENTS.md`.
