import { calculateRank, calculateWPI, calculateZScore, calculateElo, getTier, getDivision, calculateGP, calculatePercentile } from './engine.ts';
import { getSeasonalDecayMultiplier } from './constants.ts';
import { writeFileSync } from 'node:fs';

const OUT = process.argv[2];

// ---- 1. calculateRank over a broad, deterministic grid ----
let seed = 0x2f6e2b1;
const rnd = () => ((seed = (seed * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff);

const cases: any[] = [];
const push = (s: any) => cases.push({ stats: s, expected: calculateRank(s) });

// explicit edge cases
const edge = [
  [0,0,0,0,0], [1,0,0,0,0], [0,0,0,0,1],
  [0,0,0,0,999], [0,0,0,0,1000], [0,0,0,0,1001], [0,0,0,0,100000], // star cap
  [1,1,1,1,1], [10,10,10,10,10], [100,100,100,100,100],
  [50,30,20,100,250], [500,500,200,2000,1000], [5000,5000,5000,50000,1000000],
  [0,0,0,1,0], [0,1,0,0,0], [0,0,1,0,0],
];
for (const [p,r,i,c,s] of edge) push({
  totalMergedPRs:p, totalCodeReviews:r, totalIssuesClosed:i, totalCommits:c, totalStars:s,
  totalFollowers:0, firstContributionYear:2020, lastContributionYear:2026, yearsActive:7,
});

// log-uniform random spread to cover every tier
for (let n = 0; n < 3000; n++) {
  const mag = () => Math.floor(Math.pow(10, rnd() * 4.5));
  push({
    totalMergedPRs: mag(), totalCodeReviews: mag(), totalIssuesClosed: mag(),
    totalCommits: mag(), totalStars: Math.floor(Math.pow(10, rnd() * 6)),
    totalFollowers: 0, firstContributionYear: 2020, lastContributionYear: 2026, yearsActive: 7,
  });
}

// ---- 2. elo sweep: tier / division / gp at every integer elo ----
const tiers: any[] = [];
for (let elo = 0; elo <= 3400; elo++) {
  const t = getTier(elo); const d = getDivision(elo, t);
  tiers.push([elo, t, d, calculateGP(elo, t, d)]);
}

// ---- 3. percentile + z + elo sweep ----
const zs: any[] = [];
for (let i = -600; i <= 600; i++) {
  const z = i / 100;
  zs.push([z, calculatePercentile(z), calculateElo(z)]);
}

// ---- 4. wpi -> zscore sweep ----
const wpis: any[] = [];
for (let e = 0; e <= 140; e++) {
  const w = Math.pow(10, e / 10);
  wpis.push([w, calculateZScore(w)]);
}

// ---- 5. seasonal decay ----
const decay: any[] = [];
for (let cur = 2024; cur <= 2027; cur++)
  for (let y = 2010; y <= 2030; y++)
    decay.push([y, cur, getSeasonalDecayMultiplier(y, cur)]);

writeFileSync(`${OUT}/rank_cases.json`, JSON.stringify(cases, null, 0));
writeFileSync(`${OUT}/elo_sweep.json`, JSON.stringify(tiers, null, 0));
writeFileSync(`${OUT}/zscore_sweep.json`, JSON.stringify(zs, null, 0));
writeFileSync(`${OUT}/wpi_sweep.json`, JSON.stringify(wpis, null, 0));
writeFileSync(`${OUT}/decay_sweep.json`, JSON.stringify(decay, null, 0));

const dist: Record<string, number> = {};
for (const c of cases) dist[c.expected.tier] = (dist[c.expected.tier] || 0) + 1;
console.log('cases:', cases.length, '| elo sweep:', tiers.length, '| z sweep:', zs.length, '| wpi:', wpis.length, '| decay:', decay.length);
console.log('tier coverage:', dist);
