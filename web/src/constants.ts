export interface RomInfo {
  sym: string;
  repo: string;
  game: string;
}

export const KNOWN_ROMS: Record<string, RomInfo> = {
  "53d591215de2cab847d14fbcf8c516f0128cfa8556f1236065e0535aa5936d4e": {
    sym: "pokeruby.sym",
    repo: "pokeruby",
    game: "Pokemon Ruby",
  },
  "0d80909998a901c7edef5942068585bc855a85aec7e083aa6aeff84a5b2f8ec0": {
    sym: "pokeruby_rev1.sym",
    repo: "pokeruby",
    game: "Pokemon Ruby v1.1",
  },
  "0fdd36e92b75bed65d09df4635ab0b707b288c2bf1dc4c6e7a4a4f0eebe9d64c": {
    sym: "pokeruby_rev2.sym",
    repo: "pokeruby",
    game: "Pokemon Ruby v1.2",
  },
  c36c1b899503e8823ee7eb607eea583adcef7ea92ff804838b193c227f2c6657: {
    sym: "pokesapphire.sym",
    repo: "pokeruby",
    game: "Pokemon Sapphire",
  },
  "2f680a43e5c57aede4cb3b2cb04f7e15079efc122c88edaacfd6026db6e920ac": {
    sym: "pokesapphire_rev1.sym",
    repo: "pokeruby",
    game: "Pokemon Sapphire v1.1",
  },
  "02ca41513580a8b780989dee428df747b52a0b1a55bec617886b4059eb1152fb": {
    sym: "pokesapphire_rev2.sym",
    repo: "pokeruby",
    game: "Pokemon Sapphire v1.2",
  },
  a9dec84dfe7f62ab2220bafaef7479da0929d066ece16a6885f6226db19085af: {
    sym: "pokeemerald.sym",
    repo: "pokeemerald",
    game: "Pokemon Emerald",
  },
  "3d0c79f1627022e18765766f6cb5ea067f6b5bf7dca115552189ad65a5c3a8ac": {
    sym: "pokefirered.sym",
    repo: "pokefirered",
    game: "Pokemon FireRed",
  },
  "729041b940afe031302d630fdbe57c0c145f3f7b6d9b8eca5e98678d0ca4d059": {
    sym: "pokefirered_rev1.sym",
    repo: "pokefirered",
    game: "Pokemon FireRed v1.1",
  },
  "78d310d557ceebc593bd393acc52d1b19a8f023fec40bc200e6063880d8531fc": {
    sym: "pokeleafgreen.sym",
    repo: "pokefirered",
    game: "Pokemon LeafGreen",
  },
  "2f978f635b9593f6ca26ec42481c53a6b39f6cddd894ad5c062c1419fac58825": {
    sym: "pokeleafgreen_rev1.sym",
    repo: "pokefirered",
    game: "Pokemon LeafGreen v1.1",
  },
};

export interface SymResult {
  filename: string;
  symFilename: string;
  gameName: string;
}
