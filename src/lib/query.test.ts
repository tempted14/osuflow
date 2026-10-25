import { describe, expect, it } from "vitest";
import { NO_QUERY, fieldsMatch, mapMatches, parseQuery, queryNarrows, withoutChip } from "./query";
import { NO_FILTERS, hasDifficultyIn, matchesFilters, narrowsResults, visibleResults } from "./results";
import type { Beatmap, Beatmapset } from "./types";

const difficulty = (patch: Partial<Beatmap> = {}) =>
  ({
    difficulty_rating: 5,
    ar: 9,
    cs: 4,
    accuracy: 8,
    drain: 5,
    bpm: 180,
    total_length: 120,
    mode_int: 0,
    ...patch,
  }) as Beatmap;

const setOf = (beatmaps: Beatmap[], patch: Partial<Beatmapset> = {}) =>
  ({
    id: 1,
    nsfw: false,
    artist: "Camellia",
    artist_unicode: "Camellia",
    title: "Exit This Earth's Atomosphere",
    title_unicode: "Exit This Earth's Atomosphere",
    creator: "Sotarks",
    beatmaps,
    ...patch,
  }) as Beatmapset;

describe("parsing the search box", () => {
  it("leaves ordinary words untouched, spacing included", () => {
    const query = parseQuery("  freedom   dive ");
    expect(query.text).toBe("  freedom   dive ");
    expect(query.numeric).toEqual([]);
    expect(query.chips).toEqual([]);
    expect(query).not.toBe(NO_QUERY);
  });

  it("pulls a filter out and sends only the words to the mirror", () => {
    const query = parseQuery("camellia stars>5 bpm>=180");
    expect(query.text).toBe("camellia");
    expect(query.numeric.map((c) => c.key)).toEqual(["stars", "bpm"]);
    expect(query.chips.map((chip) => chip.label)).toEqual(["Stars > 5", "BPM ≥ 180"]);
  });

  it("knows the usual aliases and ignores case", () => {
    expect(parseQuery("SR>=5").numeric[0].key).toBe("stars");
    expect(parseQuery("star>5").numeric[0].key).toBe("stars");
    expect(parseQuery("dr<5").numeric[0].key).toBe("hp");
    expect(parseQuery("drain<5").numeric[0].key).toBe("hp");
    expect(parseQuery("key=7").numeric[0].key).toBe("keys");
    expect(parseQuery("len<90").numeric[0].key).toBe("length");
    expect(parseQuery("OD>8").numeric[0].key).toBe("od");
  });

  it("reads the comparison operators with their open and closed ends", () => {
    expect(parseQuery("stars>5").numeric[0]).toMatchObject({ min: 5, max: Infinity, minOpen: true });
    expect(parseQuery("stars>=5").numeric[0]).toMatchObject({ min: 5, minOpen: false });
    expect(parseQuery("ar<9.5").numeric[0]).toMatchObject({ min: -Infinity, max: 9.5, maxOpen: true });
    expect(parseQuery("ar<=9.5").numeric[0]).toMatchObject({ max: 9.5, maxOpen: false });
  });

  it("treats = and : as 'about this much', a little either side", () => {
    const ar = parseQuery("ar=9").numeric[0];
    expect(ar.min).toBeCloseTo(8.95);
    expect(ar.max).toBeCloseTo(9.05);
    expect(parseQuery("ar:9").numeric[0]).toEqual(ar);
    expect(parseQuery("ar==9").numeric[0]).toEqual(ar);
  });

  it("reads a range between two dots, whichever way round", () => {
    expect(parseQuery("bpm=180..200").numeric[0]).toMatchObject({ min: 180, max: 200, minOpen: false, maxOpen: false });
    expect(parseQuery("bpm=200..180").numeric[0]).toMatchObject({ min: 180, max: 200 });
    expect(parseQuery("stars=5..6").chips[0].label).toBe("Stars 5–6");
  });

  it("reads lengths as seconds, minutes, hours or m:ss", () => {
    expect(parseQuery("length>90").numeric[0].min).toBe(90);
    expect(parseQuery("length>90s").numeric[0].min).toBe(90);
    expect(parseQuery("length<3m").numeric[0].max).toBe(180);
    expect(parseQuery("length<1.5h").numeric[0].max).toBe(5400);
    expect(parseQuery("length>3:30").numeric[0].min).toBe(210);
    expect(parseQuery("length=2m").numeric[0]).toMatchObject({ min: 90, max: 150 });
  });

  it("accepts a star sign after the number", () => {
    expect(parseQuery("stars>5★").numeric[0].min).toBe(5);
    expect(parseQuery("stars>5*").numeric[0].min).toBe(5);
  });

  it("takes artist, title and mapper names, quoted when they have spaces", () => {
    const query = parseQuery('creator="Some Name" artist:camellia');
    expect(query.fields).toEqual([
      { key: "creator", value: "Some Name" },
      { key: "artist", value: "camellia" },
    ]);
    expect(query.text).toBe("Some Name camellia");
    expect(parseQuery("mapper=sotarks").fields[0].key).toBe("creator");
    expect(parseQuery("creator=sotarks").chips[0].label).toBe("Mapper: sotarks");
  });

  it("keeps a quote's contents together as one word", () => {
    const query = parseQuery('"stars>5 is not a filter" ar>9');
    expect(query.text).toBe('"stars>5 is not a filter"');
    expect(query.numeric).toHaveLength(1);
  });

  it("leaves words that only look like filters as words", () => {
    expect(parseQuery("Re:Zero").text).toBe("Re:Zero");
    expect(parseQuery("https://osu.ppy.sh/beatmapsets/1234").numeric).toEqual([]);
    expect(parseQuery("stars").numeric).toEqual([]);
    expect(parseQuery("artist<x").fields).toEqual([]);
    expect(parseQuery("3:30 album").text).toBe("3:30 album");
  });

  it("keeps a half-typed filter out of the search instead of sending it as a word", () => {
    for (const typed of ["camellia stars>", "camellia stars>=", "camellia ar=", "camellia bpm=18.."]) {
      const query = parseQuery(typed);
      expect(query.text).toBe("camellia");
      expect(query.numeric).toEqual([]);
    }
    expect(parseQuery("camellia stars>").ignored).toEqual([]);
  });

  it("reports a filter with a value it can't read, once the person has moved on", () => {
    expect(parseQuery("ar=abc camellia").ignored).toEqual(["ar=abc"]);
    expect(parseQuery("ar=abc camellia").text).toBe("camellia");
    // Still being typed: no complaint yet.
    expect(parseQuery("camellia length=3:").ignored).toEqual([]);
    // A space after it means it is finished.
    expect(parseQuery("camellia length=3: ").ignored).toEqual(["length=3:"]);
  });

  it("can take a filter back out of the text", () => {
    const text = "camellia stars>5 bpm>=180";
    const query = parseQuery(text);
    expect(withoutChip(text, query.chips[0])).toBe("camellia bpm>=180");
    expect(withoutChip(text, query.chips[1])).toBe("camellia stars>5");
    expect(withoutChip("stars>5", parseQuery("stars>5").chips[0])).toBe("");
  });

  it("says whether anything typed narrows the results", () => {
    expect(queryNarrows(NO_QUERY)).toBe(false);
    expect(queryNarrows(parseQuery("stars>5"))).toBe(true);
    expect(queryNarrows(parseQuery("creator=x"))).toBe(true);
    expect(queryNarrows(parseQuery("stars>"))).toBe(false);
    expect(narrowsResults(NO_FILTERS, parseQuery("ar>9"))).toBe(true);
    expect(narrowsResults(NO_FILTERS)).toBe(false);
  });
});

describe("matching difficulties", () => {
  it("compares each property with its own field", () => {
    const hard = difficulty({ difficulty_rating: 6.2, ar: 9.7, cs: 4.2, accuracy: 9.3, drain: 5.5, bpm: 150, total_length: 317 });
    expect(mapMatches(hard, parseQuery("stars>6").numeric)).toBe(true);
    expect(mapMatches(hard, parseQuery("stars<6").numeric)).toBe(false);
    expect(mapMatches(hard, parseQuery("ar>=9.7").numeric)).toBe(true);
    expect(mapMatches(hard, parseQuery("cs<4").numeric)).toBe(false);
    expect(mapMatches(hard, parseQuery("od>9").numeric)).toBe(true);
    expect(mapMatches(hard, parseQuery("hp=5.5").numeric)).toBe(true);
    expect(mapMatches(hard, parseQuery("bpm=140..160").numeric)).toBe(true);
    expect(mapMatches(hard, parseQuery("length>5m").numeric)).toBe(true);
    expect(mapMatches(hard, parseQuery("length<5m").numeric)).toBe(false);
  });

  it("makes > exclude the value and >= include it", () => {
    expect(mapMatches(difficulty({ ar: 9 }), parseQuery("ar>9").numeric)).toBe(false);
    expect(mapMatches(difficulty({ ar: 9 }), parseQuery("ar>=9").numeric)).toBe(true);
    expect(mapMatches(difficulty({ ar: 9 }), parseQuery("ar<9").numeric)).toBe(false);
    expect(mapMatches(difficulty({ ar: 9 }), parseQuery("ar<=9").numeric)).toBe(true);
  });

  it("matches the star rating as shown, to two decimals, when asked for an exact value", () => {
    expect(mapMatches(difficulty({ difficulty_rating: 5.003 }), parseQuery("stars=5").numeric)).toBe(true);
    expect(mapMatches(difficulty({ difficulty_rating: 5.2 }), parseQuery("stars=5").numeric)).toBe(false);
  });

  it("counts keys only on mania maps", () => {
    const query = parseQuery("keys=4").numeric;
    expect(mapMatches(difficulty({ mode_int: 3, cs: 4 }), query)).toBe(true);
    expect(mapMatches(difficulty({ mode_int: 3, cs: 7 }), query)).toBe(false);
    expect(mapMatches(difficulty({ mode_int: 0, cs: 4 }), query)).toBe(false);
  });

  it("needs every filter met by the same difficulty, as in the game", () => {
    const beatmaps = [
      difficulty({ difficulty_rating: 5.2, ar: 9.2 }),
      difficulty({ difficulty_rating: 3.2, ar: 7.5 }),
    ];
    const both = (text: string) => hasDifficultyIn({ beatmaps }, "any", null, parseQuery(text).numeric);
    expect(both("stars>5 ar>9")).toBe(true);
    // One difficulty is over 5 stars and another has AR under 8, but none is both.
    expect(both("stars>5 ar<8")).toBe(false);
    expect(both("stars<4 ar<8")).toBe(true);
  });

  it("combines with the difficulty band from the Filters panel on the same difficulty", () => {
    const beatmaps = [
      difficulty({ difficulty_rating: 5.8, ar: 9.5 }),
      difficulty({ difficulty_rating: 2.4, ar: 9.8 }),
    ];
    // Expert (5.3 to 6.5 stars) with AR over 9.6: the 5.8 star difficulty has AR 9.5, the 2.4 one is too easy.
    expect(hasDifficultyIn({ beatmaps }, "expert", null, parseQuery("ar>9.6").numeric)).toBe(false);
    expect(hasDifficultyIn({ beatmaps }, "expert", null, parseQuery("ar>9.4").numeric)).toBe(true);
    expect(hasDifficultyIn({ beatmaps }, "any", null, parseQuery("ar>9.6").numeric)).toBe(true);
  });

  it("only looks at the chosen game mode's difficulties", () => {
    const beatmaps = [difficulty({ mode_int: 0, difficulty_rating: 6.5 }), difficulty({ mode_int: 1, difficulty_rating: 2 })];
    const over6 = parseQuery("stars>6").numeric;
    expect(hasDifficultyIn({ beatmaps }, "any", 0, over6)).toBe(true);
    expect(hasDifficultyIn({ beatmaps }, "any", 1, over6)).toBe(false);
  });
});

describe("matching names", () => {
  it("looks for the text inside the field, in either spelling and any case", () => {
    const set = setOf([], { artist: "Camellia", artist_unicode: "かめりあ", creator: "Sotarks", title: "Exit", title_unicode: "出口" });
    expect(fieldsMatch(set, parseQuery("artist=camel").fields)).toBe(true);
    expect(fieldsMatch(set, parseQuery("artist=かめ").fields)).toBe(true);
    expect(fieldsMatch(set, parseQuery("mapper=SOTA").fields)).toBe(true);
    expect(fieldsMatch(set, parseQuery("title=出口").fields)).toBe(true);
    expect(fieldsMatch(set, parseQuery("creator=someone").fields)).toBe(false);
    expect(fieldsMatch(set, parseQuery("artist=camel creator=nobody").fields)).toBe(false);
  });
});

describe("with the rest of the filtering", () => {
  it("narrows what the grid shows, and leaves the mirrors' results alone when nothing is typed", () => {
    const easy = setOf([difficulty({ difficulty_rating: 2 })], { id: 1 });
    const hard = setOf([difficulty({ difficulty_rating: 6 })], { id: 2 });
    const none = new Set<number>();
    expect(visibleResults([easy, hard], none, false, none, NO_FILTERS, null).map((s) => s.id)).toEqual([1, 2]);
    expect(visibleResults([easy, hard], none, false, none, NO_FILTERS, null, parseQuery("stars>5")).map((s) => s.id)).toEqual([2]);
    expect(matchesFilters(hard, NO_FILTERS, null, parseQuery("stars>7"))).toBe(false);
  });
});
