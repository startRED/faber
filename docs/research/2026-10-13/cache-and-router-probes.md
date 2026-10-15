# Raw cache_probe and route_eval output (13/10/2026)

Read with docs/design.md, "Cache and router, measured on 13/10/2026".

### claude-turns (13:06)
| scenario | turn | prompt tokens | cache read | cache written | 1h written | hit | output | note |
|---|---|---|---|---|---|---|---|---|
| claude-turns | turn 1 | 11408 | 0 | 11406 | 11406 | 0.0% | 4 |  |
| claude-turns | turn 2 | 15174 | 10876 | 4296 | 4296 | 71.7% | 4 | incomplete |
| claude-turns | turn 3 | 18940 | 15147 | 3791 | 3791 | 80.0% | 4 | incomplete |

### claude-resume 0 (13:06)
| scenario | turn | prompt tokens | cache read | cache written | 1h written | hit | output | note |
|---|---|---|---|---|---|---|---|---|
| claude-resume | turn 1 | 11411 | 10876 | 533 | 533 | 95.3% | 4 |  |
| claude-resume | resumed after 0 s | 15177 | 10876 | 4299 | 4299 | 71.7% | 4 |  |

### claude-effort (13:06)
| scenario | turn | prompt tokens | cache read | cache written | 1h written | hit | output | note |
|---|---|---|---|---|---|---|---|---|
| claude-effort | medium | 11408 | 10876 | 530 | 530 | 95.3% | 4 |  |
| claude-effort | high, resumed | 15174 | 10876 | 4296 | 4296 | 71.7% | 4 |  |

### claude-effort (13:06)
| scenario | turn | prompt tokens | cache read | cache written | 1h written | hit | output | note |
|---|---|---|---|---|---|---|---|---|
| claude-effort | medium | 11407 | 10876 | 529 | 529 | 95.3% | 4 |  |
| claude-effort | high, resumed | 15173 | 10876 | 4295 | 4295 | 71.7% | 4 |  |

### codex-turns (13:06)
| scenario | turn | prompt tokens | cache read | cache written | 1h written | hit | output | note |
|---|---|---|---|---|---|---|---|---|
| codex-turns | turn 1 | 18247 | 7936 | 0 | 0 | 43.5% | 5 |  |
| codex-turns | turn 2 | 20310 | 6912 | 0 | 0 | 34.0% | 5 |  |
| codex-turns | turn 3 | 22373 | 19200 | 0 | 0 | 85.8% | 5 |  |

### codex-effort (13:07)
| scenario | turn | prompt tokens | cache read | cache written | 1h written | hit | output | note |
|---|---|---|---|---|---|---|---|---|
| codex-effort | turn 1 | 18247 | 7936 | 0 | 0 | 43.5% | 5 |  |
| codex-effort | medium, same thread | 20310 | 3840 | 0 | 0 | 18.9% | 5 |  |

### codex-effort (13:07)
| scenario | turn | prompt tokens | cache read | cache written | 1h written | hit | output | note |
|---|---|---|---|---|---|---|---|---|
| codex-effort | turn 1 | 18247 | 7936 | 0 | 0 | 43.5% | 5 |  |
| codex-effort | medium, same thread | 20310 | 6912 | 0 | 0 | 34.0% | 5 |  |

### claude-resume 240 (13:07)
| scenario | turn | prompt tokens | cache read | cache written | 1h written | hit | output | note |
|---|---|---|---|---|---|---|---|---|
| claude-resume | turn 1 | 11407 | 10876 | 529 | 529 | 95.3% | 4 |  |
| claude-resume | resumed after 240 s | 15173 | 10876 | 4295 | 4295 | 71.7% | 4 |  |

### claude-resume 360 (13:11)
| scenario | turn | prompt tokens | cache read | cache written | 1h written | hit | output | note |
|---|---|---|---|---|---|---|---|---|
| claude-resume | turn 1 | 11410 | 10876 | 532 | 532 | 95.3% | 4 |  |
| claude-resume | resumed after 360 s | 15176 | 10876 | 4298 | 4298 | 71.7% | 4 |  |

### codex-gap 6 (13:18)
| scenario | turn | prompt tokens | cache read | cache written | 1h written | hit | output | note |
|---|---|---|---|---|---|---|---|---|
| codex-gap | turn 1 | 18247 | 7936 | 0 | 0 | 43.5% | 5 |  |
| codex-gap | after 6 min | 20310 | 17152 | 0 | 0 | 84.5% | 5 |  |

### claude-resume 3700 (13:24)
| scenario | turn | prompt tokens | cache read | cache written | 1h written | hit | output | note |
|---|---|---|---|---|---|---|---|---|
| claude-resume | turn 1 | 11407 | 10876 | 529 | 529 | 95.3% | 4 |  |
| claude-resume | resumed after 3700 s | 15173 | 0 | 15171 | 15171 | 0.0% | 4 |  |

### codex-gap 35 (14:26)
| scenario | turn | prompt tokens | cache read | cache written | 1h written | hit | output | note |
|---|---|---|---|---|---|---|---|---|
| codex-gap | turn 1 | 18247 | 7936 | 0 | 0 | 43.5% | 5 |  |
| codex-gap | after 35 min | 20310 | 17152 | 0 | 0 | 84.5% | 5 |  |

### route_eval (one run)
| router | effort | valid JSON | kind | difficulty | check | median s | slowest s |
|---|---|---|---|---|---|---|---|
| gpt-6-luna | medium | 100% | 93% | 70% | 87% | 3.3 | 9.1 |
| gpt-6-luna | low | 100% | 90% | 70% | 83% | 3.3 | 4.5 |
| claude-haiku-5-5 | medium | 100% | 100% | 70% | 73% | 9.6 | 28.7 |
| claude-haiku-5-5 | low | 100% | 100% | 70% | 77% | 9.5 | 19.7 |

```text
  gpt-6-luna medium: check machine (label glance): Add a dark mode toggle to the settings page of my React app
  gpt-6-luna medium: check blind (label glance): Explain how vaccines train the immune system, for a 12-year-old
  gpt-6-luna medium: check blind (label machine): Organize these 200 receipts into a spreadsheet by month and category
  gpt-6-luna medium: check machine (label glance): Me lembra de ligar pro dentista amanhã às 10h
  gpt-6-luna low: check blind (label machine): Refactor the payment module to use the new billing API across all 14 services without breaking anything
  gpt-6-luna low: check glance (label blind): Review this function for race conditions: it updates a shared cache from several threads without a lock
  gpt-6-luna low: check blind (label glance): Explain how vaccines train the immune system, for a 12-year-old
  gpt-6-luna low: check blind (label machine): Organize these 200 receipts into a spreadsheet by month and category
  gpt-6-luna low: check machine (label glance): Me lembra de ligar pro dentista amanhã às 10h
  claude-haiku-5-5 medium: check glance (label blind): What's the capital of Australia?
  claude-haiku-5-5 medium: check glance (label blind): Monta um plano de estudos de 8 semanas pra eu passar no exame de inglês B2
  claude-haiku-5-5 medium: check blind (label machine): Refactor the payment module to use the new billing API across all 14 services without breaking anything
  claude-haiku-5-5 medium: check glance (label blind): Meu script de backup em bash às vezes apaga a pasta errada, dá uma olhada
  claude-haiku-5-5 medium: check glance (label blind): Is this SQL query vulnerable to injection? SELECT * FROM users WHERE name = '" + name + "'
  claude-haiku-5-5 medium: check glance (label machine): Quanto é 17% de 2.340?
  claude-haiku-5-5 medium: check glance (label blind): What is the expected number of rolls of a fair die until two sixes appear in a row?
  claude-haiku-5-5 medium: check glance (label machine): Organize these 200 receipts into a spreadsheet by month and category
  claude-haiku-5-5 low: check glance (label blind): What's the capital of Australia?
  claude-haiku-5-5 low: check glance (label blind): Monta um plano de estudos de 8 semanas pra eu passar no exame de inglês B2
  claude-haiku-5-5 low: check blind (label machine): Refactor the payment module to use the new billing API across all 14 services without breaking anything
  claude-haiku-5-5 low: check glance (label machine): Find all real solutions of x^4 - 5x^2 + 4 = 0 and check them
  claude-haiku-5-5 low: check glance (label blind): What is the expected number of rolls of a fair die until two sixes appear in a row?
  claude-haiku-5-5 low: check blind (label glance): Explain how vaccines train the immune system, for a 12-year-old
  claude-haiku-5-5 low: check glance (label machine): Organize these 200 receipts into a spreadsheet by month and category
```
