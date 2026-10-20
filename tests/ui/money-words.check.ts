import { de } from '../../ui/locales/de.ts';
import { en } from '../../ui/locales/en.ts';
import { es } from '../../ui/locales/es.ts';
import { fr } from '../../ui/locales/fr.ts';
import { ptBR } from '../../ui/locales/pt-BR.ts';
import { check, ok } from './check.ts';

check('money: every amount Faber counts is said one way, "in API value", in every language', () => {
  for (const [name, words] of Object.entries({ en, ptBR, es, fr, de })) {
    const own = words['answer.cost'].replace('{cost}', '').trim();
    ok(words['usage.ownSpend'].endsWith(own), `${name}: the day's total says it like a reply does ("${own}")`);
    ok(/API/.test(own), `${name}: it names the API it is measured against`);
  }
});
