---
status: accepted
---
# Complete Trajectory capture is opt-in

A captura semântica sanitizada da Trajectory continua sempre ativa. Capturar argumentos e resultados originais completos para diagnóstico exige opt-in explícito do usuário, porque esses dados podem conter credenciais, variáveis de ambiente e conteúdo privado que a normalização atual não preserva.

A fonte completa fica isolada por perfil no device executor, separada do read model sanitizado e do Run Journal de recovery, com acesso local controlado e retenção limitada. Habilitar captura não revela conteúdo: Raw Reveal continua explícito e efêmero, fora de watch, sync e Chat Transcript Export. Campos não capturados no histórico permanecem indisponíveis; não são reconstruídos a partir do estado atual do runtime.

Esta decisão complementa ADR 0004 sem alterar a captura sempre ativa dos eventos sanitizados nem autorizar cópia do ambiente inteiro do processo ou de arquivos de credenciais.
