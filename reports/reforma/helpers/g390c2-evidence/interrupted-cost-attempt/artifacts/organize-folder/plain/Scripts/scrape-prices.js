const url = process.argv[2];
fetch(url).then((r) => r.text()).then((t) => console.log(t.length));
