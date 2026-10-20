/// A later edit while the disk is saving must remain unsaved and must not join the model.
#[tokio::test]
async fn a_successful_save_learns_its_sent_links_without_learning_a_newer_unsaved_edit() {
    tokio::time::timeout(Duration::from_secs(20), learns_the_sent_links())
        .await
        .expect("the confirmed-save suggestion scenario must finish");
}

async fn learns_the_sent_links() {
    use tc_api::align::SuggestStatus;
    use tc_api::prefs::SetAlignSuggestions;
    let (_dir, t, backend) = start("sample").await;
    let at = opened(&t.app).await.session.at;
    ready(&t.app, &at, 1, 2).await;
    ok(
        &t.app,
        &SetAlignSuggestions {
            at: at.clone(),
            request: request(90),
            on: true,
        },
    )
    .await;
    tc_app_test_wait(&t.app, &at).await;
    let view = ready(&t.app, &at, 1, 1).await;
    let learned_before = suggest_events(&t.app).len();
    backend
        .script(
            &folder(SAMPLE),
            Scripted::After(Duration::from_secs(5), Box::new(Scripted::Written)),
        )
        .unwrap();
    backend
        .script(
            &folder(SAMPLE),
            Scripted::NotWritten("The disk is full.".into()),
        )
        .unwrap();
    tokio::time::pause();
    let sent = tokio::time::timeout(
        Duration::from_secs(1),
        place(&t.app, &at, &view, 91, "fe", "πίστιν"),
    )
    .await
    .expect("the first edit must finish before its save timer");
    // A Flush holds the aligner slot until the save finishes, so it cannot overlap the
    // next Edit. The timer starts the same sent version without holding that request lock.
    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::time::timeout(Duration::from_secs(1), async {
        while saved_links(&backend).is_empty() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("the first save must reach the delayed backend reply");
    assert_eq!(
        saved_links(&backend).len(),
        1,
        "the first version reached the disk"
    );
    let newer = tokio::time::timeout(
        Duration::from_secs(1),
        place(&t.app, &at, &sent, 94, "verdad", "ἀληθείας"),
    )
    .await
    .expect("a newer edit must finish while the first save reply waits");
    assert_eq!(
        saved_links(&backend).len(),
        1,
        "the first reply still waits"
    );
    tokio::time::advance(Duration::from_secs(5)).await;
    let saved = tokio::time::timeout(Duration::from_secs(1), flush(&t.app, &at, 93))
        .await
        .expect("the newer version's refused save must finish");
    assert!(matches!(saved, AlignSave::Failed(_)));
    tokio::time::resume();
    assert_eq!(
        saved_links(&backend).len(),
        2,
        "the later version was refused"
    );
    for _ in 0..200 {
        if suggest_events(&t.app).len() > learned_before {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        suggest_events(&t.app).len() > learned_before,
        "the confirmed save updated the model"
    );
    assert_eq!(
        suggest_events(&t.app).last(),
        Some(&SuggestStatus::Ready {
            verses: 1,
            boosted: false
        })
    );
    let mut unplaced = newer;
    for (n, (word, original)) in [(96, ("fe", "πίστιν")), (97, ("verdad", "ἀληθείας"))]
    {
        let at_card = card(&unplaced, original);
        let placed = unplaced.cards[at_card as usize]
            .placed
            .iter()
            .find(|w| w.word == word)
            .unwrap()
            .clone();
        unplaced = edit(
            &t.app,
            &at,
            &unplaced,
            n,
            AlignEdit::Unplace {
                card: at_card,
                word: placed,
            },
        )
        .await
        .unwrap();
    }
    let proposed: AlignerView = ok(
        &t.app,
        &tc_api::align::Suggest {
            at,
            request: request(98),
            aligner: unplaced.aligner,
        },
    )
    .await;
    let suggested: Vec<_> = proposed
        .cards
        .iter()
        .flat_map(|c| c.suggested.iter().map(|w| w.word.as_str()))
        .collect();
    assert!(
        suggested.contains(&"fe"),
        "the saved link must be learned: {suggested:?}"
    );
    assert!(
        !suggested.contains(&"verdad"),
        "the later unsaved link must not be learned: {suggested:?}"
    );
}
