# gtm-okr

This is a small command line app meant to fetch data from [GTMHub](https://gtmhub.com/) and print it out in markdown format. I wrote it primarily so I can quickly see current objectives and key results for my company. You'll need an API key to use it. The most useful command is `gtm-okr goals` but you can explore the help to find other things. Output for goals looks like:

```
* **2021-Q1** (2021-01-01 to 2021-03-31)
    * **Product team**
        * Deliver the new feature (47%)
            * KR: Code (4/4)
            * KR: Test (3.75/4)
            * KR: Document (0/3)
        * Improve performance (93%)
            * KR: Profile memory and CPU (4/5)
    * **Rev Team**
        * ...
```

