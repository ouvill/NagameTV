#ifndef VIEWER_COMMENT_MODEL_FACTORY_H
#define VIEWER_COMMENT_MODEL_FACTORY_H
#include <memory>
class CommentModel;
// Instantiate after the generated CommentModel definition is available.
template<typename T = CommentModel>
inline std::unique_ptr<T> makeCommentModel() { return std::make_unique<T>(); }

#endif
