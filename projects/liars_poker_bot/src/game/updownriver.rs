// Rules from https://plentifun.com/rules-to-play-up-down-river-card-game
// Players take a seat in a circle.
// A dealer is chosen among the players to shuffle the deck, and pass around 10 cards to each player.
// The players can look at their cards.
// Rest of the deck is kept in the middle with the top card turned face up. This card is not used in the game.
// The suit of that card will be considered as a trump suit for that round of the game. It means that while playing the tricks or hands, any card from that suit will beat other suits of cards.
// After the revelation of the trump card, each player bids the number of tricks he believes he can win. The bid can vary from zero to 10 as players have 10 cards each.
// After the bidding, a player sitting on the left of the dealer will begin a new trick of the round. He will draw a card from his stack and place it in the middle.
// The turn will go clockwise as every player will draw 1 card each.
// After cards from each player are drawn, the player who has the highest card in the drawn stack wins that trick or hand.
// The winner gets a chance to draw a card for a new trick.
// The game continues till all the cards are taken.
// The cards from the trump suits are used to win the hands. However, if the hand contains more than one trump card, the trump card of the highest value wins that hand.
// As the game is being played, each player needs to collect as many hands or win as many tricks as he had bid at the beginning of the round.
// At the end of one round, the scores are tallied and written on the score sheet.
// After all the 19 rounds end, the highest scorer wins the game.

pub struct UDRiverGameState {}
