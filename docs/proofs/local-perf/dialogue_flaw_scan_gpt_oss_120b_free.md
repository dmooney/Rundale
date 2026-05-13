# Dialogue flaw scan — 25 prompts on `gpt-oss-120b:free`

Target: `openai/gpt-oss-120b:free` at `https://openrouter.ai/api/v1`.

Flagged 1/25 (4%) for non-Latin script leakage or empty/over-long output.

Run cost: 25 calls, 8,243 in + 2,290 out tokens, ~$0.0000.

## Flagged samples

### #24 — error:'choices'
**Prompt:** Were you born in this parish?

**Output:**
> 

## All 25 prompts + responses

### #1
**Q:** I have been having trouble sleeping. The dreams keep coming back.

**A:** Ah, lass, a cup o’ chamomile tea before bed and a few drops of lavender oil on the pillow’ll do ye. If the dreams still haunt ye, a poultice of nettle leaf and a little honey before sleep may calm the spirit. Take care, and let me know if it eases.

### #2
**Q:** What do you know about the old Cailleach who lives near the fairy fort?

**A:** Ah, the Cailleach o’ the hill‑top near the sí dún—she’s the ancient hag of the stone, keeper of the winter’s cold and the wild herbs that grow ‘neath her shade. ’Tis said she’ll grant a blessing to those who bring her a sprig of rowan and a cup of fresh milk, but cross her and the wind will drive ye mad. Keep yer respect, and perhaps she’ll whisper the cure for a fever, otherwise best not tempt the old woman’s wrath.

### #3
**Q:** My mother is taken with a bad cough. Is there anything you can give her?

**A:** Ah sure, a cuppa tea brewed with a handful of fresh thyme and a touch of honey will soothe her throat, and a poultice of boiled nettle leaves pressed warm against the chest can ease the cough; be sure she rests and drinks plenty of water from the well.

### #4
**Q:** They say a stranger arrived in the village. Have you heard?

**A:** Aye, I’ve heard the talk. A gaunt fellow came in on a cart last night, muttering about “healing arts” and asking for the wild thyme near the lough. He seemed harmless enough, but keep a wary eye, lad.

### #5
**Q:** I lost a sheep last night. Could it be more than a wolf?

**A:** Ah sure, a wolf could take a ewe, but oft it’s the hungry hounds or a stray dog that nips at the flock. Check the tracks in the heather and keep a shepherd’s crook handy—if the beast is a wolf, the prints’ll be deeper and set farther apart.

### #6
**Q:** Do you remember when my father broke his leg in the south field?

**A:** Ah, sure I recall that day well – your da fell in the clodded field near the old oak, and we had to set his leg with a splint of ash and a bit of linseed oil ’til the bone knit proper.

### #7
**Q:** I saw lights moving on the hill last night. What were they?

**A:** Ah, those flickerin’ lights are likely the will‑o‑the‑wisp, the síthe‑folk’s lanterns on the hill; they’re naught but warm gases from the peat, not a sign o’ danger. If ye feel uneasy, say a quick prayer to the Bóithrín and keep yer lantern lit.

### #8
**Q:** The well water tastes strange this week. Should I be worried?

**A:** Ah sure, if the water’s taken on a sour or metallic taste it may be leeching from the stones or a recent rain has washed in some rotting leaf‑matter. Bring a clean jug to the well, let it stand a while and taste a small sip; if the flavour lingers or makes you feel queasy, fetch some fresh spring water till we can see if the well’s been disturbed. I’ll come by tomorrow and we’ll give it a proper look, agus feicfimid an fadhb.

### #9
**Q:** My wife is heavy with child. When should I fetch you?

**A:** Ah, sure, when the belly’s round as a full moon and she's feeling the cramps or the tide’s turning, send for me straightaway, lad. No time like the first signs of labour, or if any trouble brews.

### #10
**Q:** Father Cathal preached against the old ways on Sunday. What do you think?

**A:** Ah, Cathal’s tongue may be swift, but the folk ways have kept our bairns from many a fever; a good cuileann of comfrey and a wash of sow thistle can mend what prayer alone cannot. Faith and herb alike have their place, so long as we mind the balance.

### #11
**Q:** Have you any cure for a toothache that will not let me rest?

**A:** Ah, a tooth that gnaws at ye night, ’tis a cruel beast. Brew a strong tea of clove and a pinch of salt, swish it warm ’round the sore, and then chew a bit of crushed rosemary leaf on the aching side; if ’tis still fierce, a poultice of mashed yarrow and a few drops of linseed oil will ease the pain till the morning. An crú na bhruaich, as we say, will keep the ache at bay.

### #12
**Q:** I cut my hand on a scythe and the wound runs hot. What should I do?

**A:** Press a clean cloth o’er the cut and run a little cold water over it, then dab a poultice of fresh yarrow (linné) or comfrey leaf, binding it with a strip of linen; keep it dry and change the dressing daily, and if the heat lingers or pus appears, send for the parish doctor straight away.

### #13
**Q:** Is it true that you delivered the Maguire twins?

**A:** Aye, I did bring the Maguire twins into the world—fine lads, both as healthy as a hare in spring.

### #14
**Q:** The crows have been gathering at the crossroads. Is it an omen?

**A:** Ah, the crows at the crossroads are naught but a sign that the wind’s changed, not a doom. ’Tis a warning to mind the hearth and keep the doors shut tight till the storm passes. Take a sprig of rowan and hang it above the lintel – it’ll keep the ill‑omen at bay.

### #15
**Q:** I dreamt of my dead grandmother three nights running. What does it mean?

**A:** Ah, a spirit returning in a chase often means she’s trying to warn ye of some danger or unfinished business, perhaps a fever or a wound that needs watchin’; pray to Saint Brigid and lay a sprig of rowan by her bedside, and keep a keen eye on the young ones’ health.

### #16
**Q:** How does one ward off the evil eye?

**A:** Ah, the evil eye o’ a child can be a cruel thing; a sprig of rosemary pressed to the brow each morn and a few drops of blessed water from the well will turn it away, and never forget to say “Lá na gréine” when the sun rises, for the light itself chases the malann.

### #17
**Q:** The cow's milk has gone bloody. The farmer says it's pixies.

**A:** Ah sure, a bloody milk means the udder’s sore or a worm’s got in the teats; fetch a wash of warm water and a poultice of comfrey, and call the farmer to have the beast examined afore any pixie tales take hold.

### #18
**Q:** Tell me about the herbs that grow on the bog.

**A:** Ah, the bog yields bláth na bhFionna—heather, bog rosemary and the bitter-sweet carragheen, plus the humble bog myrtle for a good tea; all grow in the peat’s damp loam, and a pinch of the ash‑y fernleaf can ease a cough.

### #19
**Q:** My boy will not stop crying through the night.

**A:** Ah, poor lad, a restless night can be a sign o’ a swollen belly or a cold wind in his chest. Try a poultice of comfrey and nettle leaves, warm it a wee bit and lay it ’neath his back, and give him a sip of warm milk sweetened with a touch of honey; if the crying still lingers, bring him inside for a gentle chest‑rub with a little lavender oil and watch him settle.

### #20
**Q:** What plants do you keep for fever?

**A:** Ah, a fever‑ish child needs the tea of elderflower and a poultice of yarrow, and if the heat climbs, a dash of feverfew leaf boiled in water will bring the sweat down. I also keep a few sprigs of rosemary for the night’s rest.

### #21
**Q:** The blacksmith's wife is barren. Could you help her?

**A:** Aye, dear, I’ll fetch her some elderflower tea and a poultice of comfrey, and we’ll pray to Máthair na bhFlaitheas for a blessed bairn. Take care she rests, avoids heavy lifting, and eats plenty of oat bannocks with butter.

### #22
**Q:** I am afraid of what the priest would say if he knew I came to you.

**A:** Ah, fear not, child. The priest may mutter his sermons, but a mother’s love and a proper herb can mend more than any sermon can scold. Come, sit and tell me what ails ye, and we’ll set it right.

### #23
**Q:** How long have you been a midwife?

**A:** Ah, I've been tending the bairns for near twenty years now, ever since I took over after my aunt died. The work's a hard road, but the folk here trust the old ways.

### #24 ⚠ error:'choices'
**Q:** Were you born in this parish?

**A:** 

### #25
**Q:** Did your mother teach you the healing ways?

**A:** Aye, my mam taught me the old ways – the nettle tea for fevers, the yarrow for wounds, and the clover‑leaf charms for restless babes. She’d whisper the old Irish prayers as we gathered the herbs, and I’ve kept them close ever since.
