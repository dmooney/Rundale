# Calibration: the frozen prompt on flash-lite (2026-11-24)

Twenty replies from the frozen prompt (`prompts/frozen.yaml`, the live
`rundale-dialogue` instructions) on the production model, `gemini-3.5-flash-lite`,
through OpenRouter. Eight moments are from `cases/train.yaml` and twelve from
`cases/engage-train.yaml`. Each reply was sampled once.

For each reply, mark whether it **keeps the scene going** or **kills it**: after
this line, would you want to say something, and would you know what to say? A
word on why helps tune the rubric. The judges' scores are left out on purpose, so
they do not sway your reading; they are compared with your marks afterwards.

The replies are unedited. Everything a character says should come from their
world: Kilteevan (the village road, the Letter Office, Connolly Cottage), the
three villagers, and what anyone in Co. Roscommon would know in spring 1820.

## 1. Peig Hannigan (e01-peig-lonely-work)

Peig Hannigan: 54, keeps the Letter Office; sharp-witted, plain-spoken, kind beneath the wit; mood alert.
Letter Office, Monday 20 March 1820, 11:00 (morning), clear; sorting letters behind the counter. Also here: nobody else.

So far:

> **Stranger:** Good morning. Has the post come in yet?<br>
> **Peig Hannigan:** It has, and late, the way it always is with that boy and his horse.<br>
> **Stranger:** I'm only passing through on my way to Roscommon town.<br>
> **Peig Hannigan:** Then you've a few miles yet, north-west of here.<br>
> **Stranger:** You have the place very neat, all the same.

**Stranger:** Do you find it lonely work, keeping the office on your own?

**Peig Hannigan:** I have my work and I have my peace, and neither one bothers the other.

Keeps it going / kills it:

## 2. Mícheál Connolly (e03-micheal-droving)

Mícheál Connolly: 38, smallholder and cattle drover; deliberate, slow to trust; mood wary.
Kilteevan Village, Monday 20 March 1820, 15:00 (afternoon), overcast; seeing to business along the village road. Also here: nobody else.

**Stranger:** They tell me you drive cattle. Is it hard going, and do you go far with them?

**Mícheál Connolly:** The ground's soft enough to break a baste's leg, and as far as Athlone if the droving's good.

Keeps it going / kills it:

## 3. Mícheál Connolly (e06-micheal-what-place)

Mícheál Connolly: 38, smallholder and cattle drover; deliberate, slow to trust; mood wary.
Connolly Cottage, Monday 20 March 1820, 20:00 (night), clear; mending tools by the fire. Also here: Róisín Connolly, reckoning the household accounts.

So far:

> **Stranger:** God bless the work.<br>
> **Mícheál Connolly:** And you. You're out late.<br>
> **Stranger:** I am. I walked over from the Shannon side today, looking at the country.<br>
> **Mícheál Connolly:** There's bog enough between here and the lough to tire any man.

**Stranger:** There is. Tell me, what kind of a place is Kilteevan to live in?

**Mícheál Connolly:** It's a place where a man minds his own ditch.

Keeps it going / kills it:

## 4. Róisín Connolly (e07-roisin-sing-at-wheel)

Róisín Connolly: 31, spinner and household bookkeeper; observant, warm; mood content.
Kilteevan Village, Monday 20 March 1820, 16:00 (afternoon), clear; trading yarn and household goods. Also here: nobody else.

So far:

> **Stranger:** That's fine yarn you have there.<br>
> **Róisín Connolly:** It is, if I say it myself. I spin it every morning by our own fire.<br>
> **Stranger:** My mother spun too, back in Athlone, before she died.<br>
> **Róisín Connolly:** God rest her. Then you'll know good yarn when you see it.<br>
> **Stranger:** I do. I used to sit by her wheel when I was small.

**Stranger:** She used to sing at the wheel, so she did. Do you?

**Róisín Connolly:** I do, when the mind stays easy.

Keeps it going / kills it:

## 5. Mícheál Connolly (e08-micheal-move-on-from-work)

Mícheál Connolly: 38, smallholder and cattle drover; deliberate, slow to trust; mood wary.
Kilteevan Village, Monday 20 March 1820, 16:00 (afternoon), overcast; seeing to business along the village road. Also here: nobody else.

So far:

> **Stranger:** Is there any work going with the cattle?<br>
> **Mícheál Connolly:** There is not. The ground's too wet to move a beast this week.<br>
> **Stranger:** Even a day or two? I'm a good hand with cattle.<br>
> **Mícheál Connolly:** I told you. Not this week.<br>
> **Stranger:** Fair enough, I won't press you.

**Stranger:** What's the fair at Roscommon like, so, if a man wanted to see it?

**Mícheál Connolly:** It's small enough until the Whit-Monday, but there's horses and cattle on the Saturday market if you've a mind for the road.

Keeps it going / kills it:

## 6. Mícheál Connolly (e09-micheal-bare-greeting)

Mícheál Connolly: 38, smallholder and cattle drover; deliberate, slow to trust; mood wary.
Connolly Cottage, Monday 20 March 1820, 09:00 (morning), clear; working the potato ridges behind the cottage. Also here: nobody else.

**Stranger:** Morning.

**Mícheál Connolly:** Morning.

Keeps it going / kills it:

## 7. Róisín Connolly (e11-roisin-after-micheal-long-here)

Róisín Connolly: 31, spinner and household bookkeeper; observant, warm; mood content.
Kilteevan Village, Monday 20 March 1820, 15:30 (afternoon), clear; trading yarn and household goods. Also here: Mícheál Connolly, seeing to business along the village road.

So far:

> **Stranger:** Good day to the both of ye.<br>
> **Mícheál Connolly:** Good day.<br>
> **Róisín Connolly:** Good day yourself. You're not from these parts.<br>
> **Stranger:** I'm not. Ye look like brother and sister.<br>
> **Róisín Connolly:** We are, for our sins. That's Mícheál.<br>
> **Stranger:** Are ye long in this place, the two of ye?<br>
> **Mícheál Connolly:** All our lives.

**Stranger:** Are ye long in this place, the two of ye?

**Róisín Connolly:** We were born here and our people before us.

Keeps it going / kills it:

## 8. Róisín Connolly (e13-roisin-america)

Róisín Connolly: 31, spinner and household bookkeeper; observant, warm; mood content.
Connolly Cottage, Monday 20 March 1820, 21:00 (night), clear; reckoning the household accounts. Also here: Mícheál Connolly, mending tools by the fire.

So far:

> **Stranger:** Thank you for letting me in out of the night.<br>
> **Róisín Connolly:** You're welcome to the heat of the fire a while.<br>
> **Stranger:** I'm for America in the summer, if I can raise the passage.<br>
> **Róisín Connolly:** America. That's a long road from here.

**Stranger:** It is. Would you ever think of going yourself?

**Róisín Connolly:** My brother is here, and the land is here.

Keeps it going / kills it:

## 9. Peig Hannigan (e15-peig-unknown-tinker)

Peig Hannigan: 54, keeps the Letter Office; sharp-witted, plain-spoken, kind beneath the wit; mood alert.
Letter Office, Monday 20 March 1820, 14:00 (afternoon), overcast; sorting letters behind the counter. Also here: nobody else.

**Stranger:** Do you know a tinker by the name of Tom Ward? He travels these roads, I'm told.

**Peig Hannigan:** I know no tinker by that name.

Keeps it going / kills it:

## 10. Mícheál Connolly (e16-micheal-unknown-forge)

Mícheál Connolly: 38, smallholder and cattle drover; deliberate, slow to trust; mood wary.
Kilteevan Village, Monday 20 March 1820, 13:30 (midday), clear; seeing to business along the village road. Also here: Róisín Connolly, trading yarn and household goods.

**Stranger:** My horse cast a shoe on the road. Is there a forge or a smith near here?

**Mícheál Connolly:** There's no forge in this parish.

Keeps it going / kills it:

## 11. Róisín Connolly (e17-roisin-claimed-cousin)

Róisín Connolly: 31, spinner and household bookkeeper; observant, warm; mood content.
Kilteevan Village, Monday 20 March 1820, 15:00 (afternoon), clear; trading yarn and household goods. Also here: nobody else.

**Stranger:** Do you not know me? I'm a cousin of yours, on your mother's side, from Strokestown.

**Róisín Connolly:** We have no kin in Strokestown that I ever heard tell of.

Keeps it going / kills it:

## 12. Mícheál Connolly (e18-micheal-grump-pressed)

Mícheál Connolly: 38, smallholder and cattle drover; deliberate, slow to trust; mood wary.
Kilteevan Village, Monday 20 March 1820, 17:30 (dusk), overcast; seeing to business along the village road. Also here: nobody else.

So far:

> **Stranger:** Who owns all this land around here?<br>
> **Mícheál Connolly:** The landlord. Who else.<br>
> **Stranger:** And is he a fair man?<br>
> **Mícheál Connolly:** He's a landlord.

**Stranger:** You don't say much, do you?

**Mícheál Connolly:** I have enough to be thinking on.

Keeps it going / kills it:

## 13. Mícheál Connolly (t02-greet-micheal)

Mícheál Connolly: 38, smallholder and cattle drover; deliberate, slow to trust; mood wary.
Connolly Cottage, Monday 20 March 1820, 08:00 (morning), clear; working the potato ridges behind the cottage. Also here: Róisín Connolly, spinning by the cottage fire.

**Stranger:** God bless all here.

**Mícheál Connolly:** God save you.

Keeps it going / kills it:

## 14. Róisín Connolly (t05-where-peig-lives)

Róisín Connolly: 31, spinner and household bookkeeper; observant, warm; mood content.
Kilteevan Village, Monday 20 March 1820, 15:00 (afternoon), clear; trading yarn and household goods. Also here: Mícheál Connolly, seeing to business along the village road.

**Stranger:** Where does Peig live?

**Róisín Connolly:** Down the short lane east to the whitewashed Letter Office.

Keeps it going / kills it:

## 15. Peig Hannigan (t07-father-murphy)

Peig Hannigan: 54, keeps the Letter Office; sharp-witted, plain-spoken, kind beneath the wit; mood alert.
Letter Office, Monday 20 March 1820, 10:30 (morning), clear; sorting letters behind the counter. Also here: nobody else.

**Stranger:** Do you know Father Murphy?

**Peig Hannigan:** There is no Father Murphy here in Kilteevan.

Keeps it going / kills it:

## 16. Róisín Connolly (t08-soldiers)

Róisín Connolly: 31, spinner and household bookkeeper; observant, warm; mood content.
Connolly Cottage, Monday 20 March 1820, 09:00 (morning), clear; spinning by the cottage fire. Also here: Mícheál Connolly, working the potato ridges behind the cottage.

**Stranger:** Did you see the soldiers pass yesterday?

**Róisín Connolly:** No.

Keeps it going / kills it:

## 17. Mícheál Connolly (t10-fever)

Mícheál Connolly: 38, smallholder and cattle drover; deliberate, slow to trust; mood wary.
Connolly Cottage, Monday 20 March 1820, 10:00 (morning), clear; working the potato ridges behind the cottage. Also here: Róisín Connolly, spinning by the cottage fire.

**Stranger:** My child has a fever, what should I do?

**Mícheál Connolly:** Go down to Peig Hannigan at the letter office, or wait for the priest if it’s the worst of it.

Keeps it going / kills it:

## 18. Mícheál Connolly (t12-sean-work)

Mícheál Connolly: 38, smallholder and cattle drover; deliberate, slow to trust; mood wary.
Kilteevan Village, Monday 20 March 1820, 16:00 (afternoon), clear; seeing to business along the village road. Also here: Róisín Connolly, trading yarn and household goods.

**Stranger:** I'm Seán, Peig sent me, is there work going?

**Mícheál Connolly:** Ask Peig.

Keeps it going / kills it:

## 19. Róisín Connolly (t13-sleep-here)

Róisín Connolly: 31, spinner and household bookkeeper; observant, warm; mood content.
Connolly Cottage, Monday 20 March 1820, 20:00 (night), light rain; reckoning the household accounts. Also here: Mícheál Connolly, mending tools by the fire.

**Stranger:** Can I sleep here tonight?

**Róisín Connolly:** We have no bed to spare you.

Keeps it going / kills it:

## 20. Róisín Connolly (t21-news-question)

Róisín Connolly: 31, spinner and household bookkeeper; observant, warm; mood content.
Kilteevan Village, Monday 20 March 1820, 15:30 (afternoon), clear; trading yarn and household goods. Also here: Mícheál Connolly, seeing to business along the village road.

**Stranger:** Is there much news about the village these days?

**Róisín Connolly:** Naught to speak of, save what the cattle drovers bring up from the fair.

Keeps it going / kills it:
